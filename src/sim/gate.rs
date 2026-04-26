#![allow(clippy::upper_case_acronyms)]
use crate::sim::transistor::TransistorKind;
use crate::sim::transistor::{GateDescriptor, SignalGraph, SignalId};
use std::vec;

struct GateId(usize);
pub enum GateKind {
    NOT,
    NAND,
    AND,
    OR,
    NOR,
    XOR,
    XNOR,
}

pub enum UnitKind {
    Primitive(GateKind),
    Composite,
}

pub struct UnitDescriptor {
    pub name: String,
    pub kind: UnitKind,
    pub inputs: Vec<SignalId>,
    pub outputs: Vec<SignalId>,
    pub gates: Option<Vec<GateDescriptor>>, // for composites
    pub children: Option<Vec<UnitDescriptor>>, // for nested units
}

pub struct Schematic {
    pub units: Vec<UnitDescriptor>,
}

pub struct UnitId(usize);
impl Schematic {
    pub fn new() -> Self {
        Schematic { units: Vec::new() }
    }
    pub fn add_unit(&mut self, unit: UnitDescriptor) -> UnitId {
        let id = self.units.len();
        self.units.push(unit);
        UnitId(id)
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
            kind: UnitKind::Composite,
            inputs: vec![a, b],
            outputs: vec![sum.output, carry.output],
            gates: Some(vec![sum, carry]),
            children: None,
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
            kind: UnitKind::Composite,
            inputs: vec![a, b, cin],
            outputs: vec![ha2.outputs[0], cout.output],
            gates: Some(vec![cout]),
            children: Some(vec![ha1, ha2]),
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
            kind: UnitKind::Composite,
            inputs: a.iter().chain(b.iter()).copied().chain([cin]).collect(),
            outputs: vec![
                ad1.outputs[0],
                ad2.outputs[0],
                ad3.outputs[0],
                ad4.outputs[0],
                ad4.outputs[1],
            ],
            gates: None,
            children: Some(vec![ad1, ad2, ad3, ad4]),
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
            kind: UnitKind::Composite,
            inputs: vec![a, b, sel],
            outputs: vec![out.output],
            gates: Some(vec![not_sel, a_gate, b_gate, out]),
            children: None,
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
            kind: UnitKind::Composite,
            inputs: vec![a, b, c, d, sel1, sel2],
            outputs: vec![out.outputs[0]],
            gates: None,
            children: Some(vec![r1, r2, out]),
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
        let ap1 = graph.add_transistor(PMOS, set, graph.vdd(), q);
        let ap2 = graph.add_transistor(PMOS, not_q, graph.vdd(), q);
        let an1 = graph.add_transistor(NMOS, set, mid_top, q);
        let an2 = graph.add_transistor(NMOS, not_q, graph.gnd(), mid_top);
        let last1 = graph.kinds.len();
        // nand a above, nand b under
        let first2 = graph.kinds.len();
        let bp1 = graph.add_transistor(PMOS, reset, graph.vdd(), not_q);
        let bp2 = graph.add_transistor(PMOS, q, graph.vdd(), not_q);
        let bn1 = graph.add_transistor(NMOS, reset, mid_bot, not_q);
        let bn2 = graph.add_transistor(NMOS, q, graph.gnd(), mid_bot);
        let last2 = graph.kinds.len();

        let nand1 = GateDescriptor {
            kind: GateKind::NAND,
            inputs: vec![set, not_q],
            output: q,
            transistors: Some((first1..last1).collect()),
            sub_gates: None,
        };

        let nand2 = GateDescriptor {
            kind: GateKind::NAND,
            inputs: vec![reset, q],
            output: not_q,
            transistors: Some((first2..last2).collect()),
            sub_gates: None,
        };

        UnitDescriptor {
            name: "SR Latch".to_string(),
            kind: UnitKind::Composite,
            inputs: vec![set, reset],
            outputs: vec![q, not_q],
            gates: Some(vec![nand1, nand2]),
            children: None,
        }
    }
}
