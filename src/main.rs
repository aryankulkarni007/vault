use crossterm::ExecutableCommand;
use crossterm::event::{self, Event, KeyCode};
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Layout, Rect};
use std::io::{self, stdout};
mod render;
mod sim;

use crate::render::topology::{TopologyWidget, layout_gate};
use crate::sim::transistor::{SignalGraph, SignalState};

fn main() -> io::Result<()> {
    // initialisation of the circuit
    let mut signal_graph = SignalGraph::new();
    let a = signal_graph.add_signal(Some("A"));
    let b = signal_graph.add_signal(Some("B"));

    let xnor = signal_graph.xnor(a, b);
    let layout = layout_gate(&signal_graph, &xnor, "XNOR");
    signal_graph.propagate();

    // rendering loop
    enable_raw_mode()?;
    stdout().execute(EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;

    loop {
        terminal.draw(|frame| {
            let area = frame.area();
            frame.render_widget(
                TopologyWidget {
                    graph: &signal_graph,
                    layout: &layout,
                },
                area,
            );
        })?;
        if let Event::Key(key) = event::read()? {
            match key.code {
                KeyCode::Char('q') => break,
                KeyCode::Char('a') => {
                    let current = signal_graph.driven[a.0];
                    match current {
                        Some(SignalState::High) => signal_graph.drive(a, Some(SignalState::Low)),
                        Some(SignalState::Low) => signal_graph.drive(a, Some(SignalState::High)),
                        _ => signal_graph.drive(a, Some(SignalState::Low)),
                    }
                    signal_graph.propagate();
                }
                KeyCode::Char('b') => {
                    let current = signal_graph.driven[b.0];
                    match current {
                        Some(SignalState::High) => signal_graph.drive(b, Some(SignalState::Low)),
                        Some(SignalState::Low) => signal_graph.drive(b, Some(SignalState::High)),
                        _ => signal_graph.drive(b, Some(SignalState::Low)),
                    }
                    signal_graph.propagate();
                }
                _ => (),
            }
        }
    }
    disable_raw_mode()?;
    stdout().execute(LeaveAlternateScreen)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::{
        gate::Schematic,
        transistor::{
            SignalId,
            SignalState::{self, *},
        },
    };

    fn check(
        g: &mut SignalGraph,
        name: &str,
        ins: &[SignalId],
        out: SignalId,
        exp: &[SignalState],
    ) {
        println!("\nTruth Table: {}", name);
        let header = ins.iter().map(|_| "In").collect::<Vec<_>>().join(" | ");
        println!(" {} | OUT\n{}", header, "-".repeat(header.len() + 7));

        (0..(1 << ins.len())).for_each(|i| {
            let mut row = Vec::new();
            for j in 0..ins.len() {
                let state = if (i >> (ins.len() - 1 - j)) & 1 == 1 {
                    High
                } else {
                    Low
                };
                g.drive(ins[j], Some(state));
                row.push(if state == High { "1" } else { "0" });
            }
            g.propagate();
            let act = g.signals[out.0];
            println!(
                " {} |  {}",
                row.join(" | "),
                if act == High { "1" } else { "0" }
            );
            assert_eq!(act, exp[i], "Fail {} at case {}", name, i);
        });
    }

    #[test]
    fn test_gates() {
        let mut g = SignalGraph::new();
        let (a, b) = (g.add_signal(Some("A")), g.add_signal(Some("B")));

        let not_out = g.not(a).output;
        let nand_out = g.nand(a, b).output;
        let and_out = g.and(a, b).output;
        let nor_out = g.nor(a, b).output;
        let xor_out = g.xor(a, b).output;
        let xnor_out = g.xnor(a, b).output;

        check(&mut g, "NOT", &[a], not_out, &[High, Low]);
        check(&mut g, "NAND", &[a, b], nand_out, &[High, High, High, Low]);
        check(&mut g, "AND", &[a, b], and_out, &[Low, Low, Low, High]);
        check(&mut g, "NOR", &[a, b], nor_out, &[High, Low, Low, Low]);
        check(&mut g, "XOR", &[a, b], xor_out, &[Low, High, High, Low]);
        check(&mut g, "XNOR", &[a, b], xnor_out, &[High, Low, Low, High]);
    }

    #[test]
    fn test_half_adder() {
        let mut g = SignalGraph::new();
        let mut s = Schematic::new();
        let (a, b) = (g.add_signal(Some("A")), g.add_signal(Some("B")));
        let ha = s.half_adder(&mut g, a, b);
        let sum = ha.outputs[0];
        let carry = ha.outputs[1];
        check(&mut g, "HA_SUM", &[a, b], sum, &[Low, High, High, Low]);
        check(&mut g, "HA_CARRY", &[a, b], carry, &[Low, Low, Low, High]);
    }

    #[test]
    fn test_full_adder() {
        let mut g = SignalGraph::new();
        let mut s = Schematic::new();
        let a = g.add_signal(Some("A"));
        let b = g.add_signal(Some("B"));
        let cin = g.add_signal(Some("CIN"));
        let fa = s.full_adder(&mut g, a, b, cin);
        let sum = fa.outputs[0];
        let cout = fa.outputs[1];
        check(
            &mut g,
            "FA_SUM",
            &[a, b, cin],
            sum,
            &[Low, High, High, Low, High, Low, Low, High],
        );
        check(
            &mut g,
            "FA_COUT",
            &[a, b, cin],
            cout,
            &[Low, Low, Low, High, Low, High, High, High],
        );
    }

    #[test]
    fn test_adder_4bit() {
        let mut g = SignalGraph::new();
        let mut s = Schematic::new();
        let a = [
            g.add_signal(Some("A0")),
            g.add_signal(Some("A1")),
            g.add_signal(Some("A2")),
            g.add_signal(Some("A3")),
        ];
        let b = [
            g.add_signal(Some("B0")),
            g.add_signal(Some("B1")),
            g.add_signal(Some("B2")),
            g.add_signal(Some("B3")),
        ];
        let cin = g.add_signal(Some("CIN"));
        let adder = s.adder_4bit(&mut g, a, b, cin);

        // test 3 + 4 = 7, no carry
        // a = 0011, b = 0100, cin = 0
        g.drive(a[0], Some(High));
        g.drive(a[1], Some(High));
        g.drive(a[2], Some(Low));
        g.drive(a[3], Some(Low));
        g.drive(b[0], Some(Low));
        g.drive(b[1], Some(Low));
        g.drive(b[2], Some(High));
        g.drive(b[3], Some(Low));
        g.drive(cin, Some(Low));
        g.propagate();
        // expect sum = 0111 = 7, cout = 0
        assert_eq!(g.signals[adder.outputs[0].0], High, "sum bit 0");
        assert_eq!(g.signals[adder.outputs[1].0], High, "sum bit 1");
        assert_eq!(g.signals[adder.outputs[2].0], High, "sum bit 2");
        assert_eq!(g.signals[adder.outputs[3].0], Low, "sum bit 3");
        assert_eq!(g.signals[adder.outputs[4].0], Low, "cout");

        // test 15 + 1 = 16, overflow
        // a = 1111, b = 0001, cin = 0
        g.drive(a[0], Some(High));
        g.drive(a[1], Some(High));
        g.drive(a[2], Some(High));
        g.drive(a[3], Some(High));
        g.drive(b[0], Some(High));
        g.drive(b[1], Some(Low));
        g.drive(b[2], Some(Low));
        g.drive(b[3], Some(Low));
        g.drive(cin, Some(Low));
        g.propagate();
        // expect sum = 0000, cout = 1
        assert_eq!(g.signals[adder.outputs[0].0], Low, "sum bit 0");
        assert_eq!(g.signals[adder.outputs[1].0], Low, "sum bit 1");
        assert_eq!(g.signals[adder.outputs[2].0], Low, "sum bit 2");
        assert_eq!(g.signals[adder.outputs[3].0], Low, "sum bit 3");
        assert_eq!(g.signals[adder.outputs[4].0], High, "cout");
    }

    #[test]
    fn test_mux() {
        let mut g = SignalGraph::new();
        let mut s = Schematic::new();
        let a = g.add_signal(Some("A"));
        let b = g.add_signal(Some("B"));
        let sel = g.add_signal(Some("SEL"));
        let mux = s.mux(&mut g, a, b, sel);
        let out = mux.outputs[0];
        // sel=Low → output follows a
        g.drive(sel, Some(Low));
        check(&mut g, "MUX_SEL_LOW", &[a, b], out, &[Low, Low, High, High]);
        // sel=High → output follows b
        g.drive(sel, Some(High));
        check(
            &mut g,
            "MUX_SEL_HIGH",
            &[a, b],
            out,
            &[Low, High, Low, High],
        );
    }
}
