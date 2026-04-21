use crossterm::ExecutableCommand;
use crossterm::event::{self, Event, KeyCode};
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use std::io::{self, stdout};
mod render;
mod sim;

use crate::render::panel::SignalPanel;
use crate::sim::{SignalGraph, SignalState};

fn main() -> io::Result<()> {
    // initialisation of the circuit
    let mut signal_graph = SignalGraph::new();
    let a = signal_graph.add_signal(Some("A"));
    let b = signal_graph.add_signal(Some("B"));
    let _out = signal_graph.nand(a, b);

    enable_raw_mode()?;
    stdout().execute(EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;

    loop {
        terminal.draw(|frame| {
            let chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
                .split(frame.area());

            frame.render_widget(
                SignalPanel {
                    graph: &signal_graph,
                },
                chunks[1],
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
    use crate::sim::SignalState;

    #[test]
    fn nand() {
        let mut g = SignalGraph::new();
        let a = g.add_signal(Some("A"));
        let b = g.add_signal(Some("B"));
        let out = g.nand(a, b);

        // test case 1 (Low Low -> High)
        g.drive(a, Some(SignalState::Low));
        g.drive(b, Some(SignalState::Low));
        g.propagate();
        assert_eq!(g.signals[out.0], SignalState::High);

        // test case 2 (Low High -> High)
        g.drive(a, Some(SignalState::Low));
        g.drive(b, Some(SignalState::High));
        g.propagate();
        assert_eq!(g.signals[out.0], SignalState::High);

        // test case 3 (High Low -> High)
        g.drive(a, Some(SignalState::High));
        g.drive(b, Some(SignalState::Low));
        g.propagate();
        assert_eq!(g.signals[out.0], SignalState::High);

        // test case 4 (High High -> Low)
        g.drive(a, Some(SignalState::High));
        g.drive(b, Some(SignalState::High));
        g.propagate();
        assert_eq!(g.signals[out.0], SignalState::Low);
    }
}
