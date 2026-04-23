#![allow(clippy::upper_case_acronyms)]
use std::vec;

use crate::sim::transistor::{GateDescriptor, SignalGraph, SignalId};

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

enum UnitKind {
    Primitive(GateKind),
    Composite,
}

pub struct UnitDescriptor {
    pub name: String,
    pub kind: UnitKind,
    pub inputs: Vec<SignalId>,
    pub outputs: Vec<SignalId>,
    pub gates: Vec<GateDescriptor>,    // for composites
    pub children: Vec<UnitDescriptor>, // for nested units
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
            kind: UnitKind::Composite,
            inputs: vec![a, b, cin],
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
            kind: UnitKind::Composite,
            inputs: a.iter().chain(b.iter()).copied().chain([cin]).collect(),
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
}
