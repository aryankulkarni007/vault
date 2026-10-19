use crate::render::tlayout::{GateLayout, layout_gate, layout_unit};
use crate::sim::gate::{Schematic, UnitDescriptor};
use crate::sim::transistor::{GateDescriptor, SignalGraph, SignalId, SignalState};

/// One viewable circuit, in its own signal graph.
pub struct Scene {
    pub graph: SignalGraph,
    pub layout: GateLayout,
    /// toggled with keys 1-9, in this order
    pub inputs: Vec<SignalId>,
    pub clock: SignalId,
}

type GateBuilder = fn(&mut SignalGraph, &[SignalId]) -> GateDescriptor;
type UnitBuilder = fn(&mut Schematic, &mut SignalGraph, &[SignalId]) -> UnitDescriptor;

fn gate_scene(label: &str, input_names: &[&str], build: GateBuilder) -> Scene {
    let mut graph = SignalGraph::new();
    let clock = graph.add_clock(10);
    let inputs: Vec<SignalId> = input_names
        .iter()
        .map(|n| graph.add_signal(Some(n)))
        .collect();
    let gate = build(&mut graph, &inputs);
    graph.name(gate.output, "Y");
    let layout = layout_gate(&graph, &gate, label);
    settle(graph, layout, inputs, clock)
}

fn unit_scene(input_names: &[&str], output_names: &[&str], build: UnitBuilder) -> Scene {
    let mut graph = SignalGraph::new();
    let clock = graph.add_clock(10);
    let inputs: Vec<SignalId> = input_names
        .iter()
        .map(|n| graph.add_signal(Some(n)))
        .collect();
    let unit = build(&mut Schematic::new(), &mut graph, &inputs);
    for (&id, name) in unit.outputs.iter().zip(output_names) {
        graph.name(id, name);
    }
    let layout = layout_unit(&graph, &unit);
    settle(graph, layout, inputs, clock)
}

/// all inputs start LOW
fn settle(
    mut graph: SignalGraph,
    layout: GateLayout,
    inputs: Vec<SignalId>,
    clock: SignalId,
) -> Scene {
    for &i in &inputs {
        graph.drive(i, Some(SignalState::Low));
    }
    graph.propagate();
    Scene {
        graph,
        layout,
        inputs,
        clock,
    }
}

/// Every gate and unit the simulator can build, simplest first.
pub fn scenes() -> Vec<Scene> {
    let mut sr = unit_scene(&["SET", "RESET"], &["Q", "!Q"], |s, g, i| {
        s.sr_latch(g, i[0], i[1])
    });
    // the latch's inputs are active low: pulse RESET, then rest in HOLD
    let (set, reset) = (sr.inputs[0], sr.inputs[1]);
    sr.graph.drive(set, Some(SignalState::High));
    sr.graph.drive(reset, Some(SignalState::Low));
    sr.graph.propagate();
    sr.graph.drive(reset, Some(SignalState::High));
    sr.graph.propagate();

    vec![
        gate_scene("NOT", &["A"], |g, i| g.not(i[0])),
        gate_scene("NAND", &["A", "B"], |g, i| g.nand(i[0], i[1])),
        gate_scene("NOR", &["A", "B"], |g, i| g.nor(i[0], i[1])),
        gate_scene("AND", &["A", "B"], |g, i| g.and(i[0], i[1])),
        gate_scene("OR (NOR + NOT)", &["A", "B"], |g, i| g.or(i[0], i[1], true)),
        gate_scene("OR (2x NOT + NAND)", &["A", "B"], |g, i| {
            g.or(i[0], i[1], false)
        }),
        gate_scene("XOR", &["A", "B"], |g, i| g.xor(i[0], i[1])),
        gate_scene("XNOR", &["A", "B"], |g, i| g.xnor(i[0], i[1])),
        unit_scene(&["A", "B"], &["S", "C"], |s, g, i| {
            s.half_adder(g, i[0], i[1])
        }),
        unit_scene(&["A", "B", "CIN"], &["S", "COUT"], |s, g, i| {
            s.full_adder(g, i[0], i[1], i[2])
        }),
        unit_scene(
            &["A0", "A1", "A2", "A3", "B0", "B1", "B2", "B3", "CIN"],
            &["S0", "S1", "S2", "S3", "COUT"],
            |s, g, i| s.adder_4bit(g, [i[0], i[1], i[2], i[3]], [i[4], i[5], i[6], i[7]], i[8]),
        ),
        unit_scene(&["A", "B", "SEL"], &["Y"], |s, g, i| {
            s.mux_2to1(g, i[0], i[1], i[2])
        }),
        unit_scene(&["A", "B", "C", "D", "S0", "S1"], &["Y"], |s, g, i| {
            s.mux_4to1(g, i[0], i[1], i[2], i[3], i[4], i[5])
        }),
        sr,
    ]
}
