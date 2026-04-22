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

    let and = signal_graph.and(a, b);
    let layout = layout_gate(&signal_graph, &and, "and");
    signal_graph.propagate();

    enable_raw_mode()?;
    stdout().execute(EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;

    loop {
        terminal.draw(|frame| {
            let area = centered_rect(50, 70, frame.area());
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
    use crate::sim::{SignalId, SignalState};
    fn print_and_check_2_input(
        g: &mut SignalGraph,
        name: &str,
        a: SignalId,
        b: SignalId,
        out: SignalId,
        expected: [SignalState; 4],
    ) {
        println!("\nTruth Table for: {}", name);
        println!(" A | B | OUT");
        println!("-----------");

        let cases = [
            (SignalState::Low, SignalState::Low, expected[0]),
            (SignalState::Low, SignalState::High, expected[1]),
            (SignalState::High, SignalState::Low, expected[2]),
            (SignalState::High, SignalState::High, expected[3]),
        ];

        for (in_a, in_b, exp) in cases {
            g.drive(a, Some(in_a));
            g.drive(b, Some(in_b));
            g.propagate();

            let actual = g.signals[out.0];

            let fmt = |s: SignalState| if s == SignalState::High { "1" } else { "0" };
            println!(" {} | {} |  {}", fmt(in_a), fmt(in_b), fmt(actual));

            assert_eq!(
                actual, exp,
                "Failed {} gate for inputs {:?}, {:?}",
                name, in_a, in_b
            );
        }
    }

    #[test]
    fn test_not() {
        let mut g = SignalGraph::new();
        let a = g.add_signal(Some("A"));
        let out = g.not(a).output;

        g.drive(a, Some(SignalState::Low));
        g.propagate();
        assert_eq!(g.signals[out.0], SignalState::High);

        g.drive(a, Some(SignalState::High));
        g.propagate();
        assert_eq!(g.signals[out.0], SignalState::Low);
    }

    #[test]
    fn test_all_gates() {
        let mut g = SignalGraph::new();
        let a = g.add_signal(Some("A"));
        let b = g.add_signal(Some("B"));

        let nand_out = g.nand(a, b).output;
        print_and_check_2_input(
            &mut g,
            "NAND",
            a,
            b,
            nand_out,
            [
                SignalState::High,
                SignalState::High,
                SignalState::High,
                SignalState::Low,
            ],
        );

        let and_out = g.and(a, b).output;
        print_and_check_2_input(
            &mut g,
            "AND",
            a,
            b,
            and_out,
            [
                SignalState::Low,
                SignalState::Low,
                SignalState::Low,
                SignalState::High,
            ],
        );

        let nor_out = g.nor(a, b).output;
        print_and_check_2_input(
            &mut g,
            "NOR",
            a,
            b,
            nor_out,
            [
                SignalState::High,
                SignalState::Low,
                SignalState::Low,
                SignalState::Low,
            ],
        );

        let or_std_out = g.or(a, b, false).output;
        print_and_check_2_input(
            &mut g,
            "OR (Standard)",
            a,
            b,
            or_std_out,
            [
                SignalState::Low,
                SignalState::High,
                SignalState::High,
                SignalState::High,
            ],
        );

        let or_dm_out = g.or(a, b, true).output;
        print_and_check_2_input(
            &mut g,
            "OR (De Morgan)",
            a,
            b,
            or_dm_out,
            [
                SignalState::Low,
                SignalState::High,
                SignalState::High,
                SignalState::High,
            ],
        );
    }
}
