use crossterm::ExecutableCommand;
use crossterm::event::{self, Event, KeyCode};
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::{Terminal, layout};
use std::io::{self, stdout};
mod render;
mod sim;

use crate::render::panel::SignalPanel;
use crate::render::topology::{TopologyWidget, layout_gate};
use crate::sim::{SignalGraph, SignalState};

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vert = Layout::vertical([
        Constraint::Percentage((100 - percent_y) / 2),
        Constraint::Percentage(percent_y),
        Constraint::Percentage((100 - percent_y) / 2),
    ])
    .split(area);

    Layout::horizontal([
        Constraint::Percentage((100 - percent_x) / 2),
        Constraint::Percentage(percent_x),
        Constraint::Percentage((100 - percent_x) / 2),
    ])
    .split(vert[1])[1]
}

fn main() -> io::Result<()> {
    // initialisation of the circuit
    let mut signal_graph = SignalGraph::new();
    let a = signal_graph.add_signal(Some("A"));
    let b = signal_graph.add_signal(Some("B"));

    let xor = signal_graph.xor(a, b);
    let layout = layout_gate(&signal_graph, &xor, "XOR");
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

/* ai tests */
#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::{SignalId, SignalState, SignalState::*};

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
}
