#![allow(clippy::upper_case_acronyms)]
use crate::sim::transistor::{GateBody, GateDescriptor, SignalGraph, SignalId};

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum GateKind {
    NOT,
    NAND,
    AND,
    OR,
    NOR,
    XOR,
    XNOR,
}

impl GateKind {
    pub fn name(self) -> &'static str {
        match self {
            GateKind::NOT => "NOT",
            GateKind::NAND => "NAND",
            GateKind::AND => "AND",
            GateKind::OR => "OR",
            GateKind::NOR => "NOR",
            GateKind::XOR => "XOR",
            GateKind::XNOR => "XNOR",
        }
    }
}

pub struct UnitDescriptor {
    pub name: String,
    pub outputs: Vec<SignalId>,
    pub gates: Vec<GateDescriptor>, // gates built directly by this unit
    pub children: Vec<UnitDescriptor>, // nested units, built before `gates`
}

impl UnitDescriptor {
    /// Every gate in construction order (children first, then own gates),
    /// grouped by the element directly under this unit: one group per child
    /// unit, one per own gate.
    pub fn flatten(&self) -> Vec<(String, Vec<&GateDescriptor>)> {
        let mut out = Vec::new();
        for child in &self.children {
            let gates = child.flatten().into_iter().flat_map(|(_, g)| g).collect();
            out.push((child.name.clone(), gates));
        }
        out.extend(
            self.gates
                .iter()
                .map(|g| (g.kind.name().to_string(), vec![g])),
        );
        out
    }
}

/// Builders for multi-gate units. Stateless; it exists to group the builders.
pub struct Schematic;

impl Schematic {
    pub fn new() -> Self {
        Schematic
    }
}

impl Schematic {
    pub fn half_adder(
        &mut self,
        graph: &mut SignalGraph,
        a: SignalId,
        b: SignalId,
    ) -> UnitDescriptor {
        let sum = graph.xor(a, b);
        let carry = graph.and(a, b);
        UnitDescriptor {
            name: "Half Adder".to_string(),
            outputs: vec![sum.output, carry.output],
            gates: vec![sum, carry],
            children: vec![],
        }
    }
    pub fn full_adder(
        &mut self,
        graph: &mut SignalGraph,
        a: SignalId,
        b: SignalId,
        cin: SignalId,
    ) -> UnitDescriptor {
        let ha1 = self.half_adder(graph, a, b); // sum, carry
        let ha2 = self.half_adder(graph, ha1.outputs[0], cin); // sum, carry
        let cout = graph.or(ha1.outputs[1], ha2.outputs[1], true);
        UnitDescriptor {
            name: "Full Adder".to_string(),
            outputs: vec![ha2.outputs[0], cout.output],
            gates: vec![cout],
            children: vec![ha1, ha2],
        }
    }

    pub fn adder_4bit(
        &mut self,
        graph: &mut SignalGraph,
        a: [SignalId; 4],
        b: [SignalId; 4],
        cin: SignalId,
    ) -> UnitDescriptor {
        // 4 full adders chained
        let ad1 = self.full_adder(graph, a[0], b[0], cin);
        let ad2 = self.full_adder(graph, a[1], b[1], ad1.outputs[1]);
        let ad3 = self.full_adder(graph, a[2], b[2], ad2.outputs[1]);
        let ad4 = self.full_adder(graph, a[3], b[3], ad3.outputs[1]);
        UnitDescriptor {
            name: "4 bit Ripple Adder".to_string(),
            outputs: vec![
                ad1.outputs[0],
                ad2.outputs[0],
                ad3.outputs[0],
                ad4.outputs[0],
                ad4.outputs[1],
            ],
            gates: vec![],
            children: vec![ad1, ad2, ad3, ad4],
        }
    }

    pub fn mux_2to1(
        &mut self,
        graph: &mut SignalGraph,
        a: SignalId,
        b: SignalId,
        sel: SignalId,
    ) -> UnitDescriptor {
        let not_sel = graph.not(sel);
        let a_gate = graph.and(a, not_sel.output);
        let b_gate = graph.and(b, sel);
        let out = graph.or(a_gate.output, b_gate.output, true);
        UnitDescriptor {
            name: "MUX 2:1".to_string(),
            outputs: vec![out.output],
            gates: vec![not_sel, a_gate, b_gate, out],
            children: vec![],
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn mux_4to1(
        &mut self,
        graph: &mut SignalGraph,
        a: SignalId,
        b: SignalId,
        c: SignalId,
        d: SignalId,
        sel1: SignalId,
        sel2: SignalId,
    ) -> UnitDescriptor {
        let r1 = self.mux_2to1(graph, a, b, sel1);
        let r2 = self.mux_2to1(graph, c, d, sel1);
        let out = self.mux_2to1(graph, r1.outputs[0], r2.outputs[0], sel2);
        UnitDescriptor {
            name: "MUX 4:1".to_string(),
            outputs: vec![out.outputs[0]],
            gates: vec![],
            children: vec![r1, r2, out],
        }
    }

    /// high, high -> idle, hold
    /// low, high -> set
    /// high, low -> reset
    /// low, low -> undefined
    pub fn sr_latch(
        &mut self,
        graph: &mut SignalGraph,
        set: SignalId,
        reset: SignalId,
    ) -> UnitDescriptor {
        use crate::sim::transistor::TransistorKind::*;
        let q = graph.add_signal(Some("Q"));
        let not_q = graph.add_signal(Some("!Q"));
        let mid_top = graph.add_signal(None);
        let mid_bot = graph.add_signal(None);

        let first1 = graph.kinds.len();
        let _ap1 = graph.add_transistor(PMOS, set, graph.vdd(), q);
        let _ap2 = graph.add_transistor(PMOS, not_q, graph.vdd(), q);
        let _an1 = graph.add_transistor(NMOS, set, mid_top, q);
        let _an2 = graph.add_transistor(NMOS, not_q, graph.gnd(), mid_top);
        let last1 = graph.kinds.len();
        // nand a above, nand b under
        let first2 = graph.kinds.len();
        let _bp1 = graph.add_transistor(PMOS, reset, graph.vdd(), not_q);
        let _bp2 = graph.add_transistor(PMOS, q, graph.vdd(), not_q);
        let _bn1 = graph.add_transistor(NMOS, reset, mid_bot, not_q);
        let _bn2 = graph.add_transistor(NMOS, q, graph.gnd(), mid_bot);
        let last2 = graph.kinds.len();

        let nand1 = GateDescriptor {
            kind: GateKind::NAND,
            output: q,
            body: GateBody::Stage((first1..last1).collect()),
        };

        let nand2 = GateDescriptor {
            kind: GateKind::NAND,
            output: not_q,
            body: GateBody::Stage((first2..last2).collect()),
        };

        UnitDescriptor {
            name: "SR Latch".to_string(),
            outputs: vec![q, not_q],
            gates: vec![nand1, nand2],
            children: vec![],
        }
    }
}
