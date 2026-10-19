use std::io;
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
mod gallery;
mod render;
mod sim;

use crate::render::panel::SignalPanel;
use crate::render::tlayout::TopologyWidget;
use crate::sim::transistor::SignalState;

#[derive(PartialEq)]
enum TickMode {
    Auto(u64),
    Paused,
}

const PAN_STEP: u16 = 8;

fn main() -> io::Result<()> {
    let mut scenes = gallery::scenes();
    let total = scenes.len();
    let mut current = 0;
    let mut scroll_x: u16 = 0;

    // init() also installs a panic hook that restores the terminal, so a
    // crash never leaves the shell stuck in raw mode
    let mut terminal = ratatui::init();

    let mut tick_mode = TickMode::Paused;
    let mut last_tick = Instant::now();

    let result = loop {
        let scene = &mut scenes[current];

        // Auto-tick check, runs every loop iteration
        if let TickMode::Auto(interval) = tick_mode
            && last_tick.elapsed().as_millis() as u64 >= interval
        {
            scene.graph.tick();
            last_tick = Instant::now();
        }

        let drawn = terminal.draw(|frame| {
            let rows = Layout::default()
                .direction(ratatui::layout::Direction::Vertical)
                .constraints([Constraint::Min(3), Constraint::Length(1)])
                .split(frame.area());
            let cols = Layout::default()
                .direction(ratatui::layout::Direction::Horizontal)
                .constraints([Constraint::Min(20), Constraint::Length(22)])
                .split(rows[0]);

            frame.render_widget(
                TopologyWidget {
                    graph: &scene.graph,
                    layout: &scene.layout,
                    scroll_x,
                },
                cols[0],
            );
            frame.render_widget(
                SignalPanel {
                    graph: &scene.graph,
                    inputs: &scene.inputs,
                },
                cols[1],
            );

            let clk_str = if scene.graph.signals[scene.clock.0] == SignalState::High {
                "CLK: HIGH"
            } else {
                "CLK: LOW "
            };
            let mode_str = match tick_mode {
                TickMode::Paused => "PAUSED".to_string(),
                TickMode::Auto(ms) => format!("AUTO {}ms", ms),
            };
            let settle_str = if scene.graph.settled {
                "SETTLED"
            } else {
                "UNSTABLE"
            };
            let status = format!(
                " {}/{} | {} stages, {} transistors | {} | {} | {} | cycle: {:>6} | [1-9]inputs [tab]circuit [←→]pan [t]step [a]slow [s]slower [f]fast [space]pause [q]quit",
                current + 1,
                total,
                scene.layout.stages.len(),
                scene.layout.devices().len(),
                clk_str,
                mode_str,
                settle_str,
                scene.graph.cycle_count
            );
            let status_style = Style::default().fg(Color::Rgb(96, 136, 96));
            frame.render_widget(Line::from(Span::styled(status, status_style)), rows[1]);
        });
        if let Err(e) = drawn {
            break Err(e);
        }

        match event::poll(Duration::from_millis(16)) {
            Ok(false) => continue,
            Ok(true) => {}
            Err(e) => break Err(e),
        }
        let key = match event::read() {
            Ok(Event::Key(key)) if key.kind == KeyEventKind::Press => key,
            Ok(_) => continue,
            Err(e) => break Err(e),
        };
        let scene = &mut scenes[current];
        match key.code {
            KeyCode::Char('q') => break Ok(()),

            KeyCode::Char(c @ '1'..='9') => {
                let index = c as usize - '1' as usize;
                if let Some(&input) = scene.inputs.get(index) {
                    let next = match scene.graph.driven[input.0] {
                        Some(SignalState::Low) => SignalState::High,
                        _ => SignalState::Low,
                    };
                    scene.graph.drive(input, Some(next));
                    scene.graph.propagate();
                }
            }

            KeyCode::Tab => {
                current = (current + 1) % scenes.len();
                scroll_x = 0;
            }
            KeyCode::BackTab => {
                current = (current + scenes.len() - 1) % scenes.len();
                scroll_x = 0;
            }
            KeyCode::Left | KeyCode::Char('h') => scroll_x = scroll_x.saturating_sub(PAN_STEP),
            KeyCode::Right | KeyCode::Char('l') => {
                scroll_x = (scroll_x + PAN_STEP).min(scene.layout.width());
            }

            KeyCode::Char('t') => {
                scene.graph.tick();
            }
            KeyCode::Char('a') => {
                tick_mode = TickMode::Auto(100);
                last_tick = Instant::now();
            }
            KeyCode::Char('s') => {
                tick_mode = TickMode::Auto(500);
                last_tick = Instant::now();
            }
            KeyCode::Char('f') => {
                tick_mode = TickMode::Auto(16);
                last_tick = Instant::now();
            }
            KeyCode::Char(' ') => {
                tick_mode = TickMode::Paused;
            }

            _ => (),
        }
    };

    ratatui::restore();
    result
}

#[cfg(test)]
mod tests {
    use crate::sim::gate::Schematic;
    use crate::sim::transistor::SignalGraph;
    use crate::sim::transistor::SignalId;
    use crate::sim::transistor::SignalState::{self, *};

    fn check(g: &mut SignalGraph,
             name: &str,
             ins: &[SignalId],
             out: SignalId,
             exp: &[SignalState]) {
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
                                 println!(" {} |  {}",
                                          row.join(" | "),
                                          if act == High { "1" } else { "0" });
                                 assert_eq!(act, exp[i], "Fail {} at case {}", name, i);
                                 // Cleanup: undrive inputs so they don't affect next row
                                 for &input in ins {
                                     g.drive(input, None);
                                 }
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
        let or_out = g.or(a, b, true).output;
        let or_nand_out = g.or(a, b, false).output;

        check(&mut g, "NOT", &[a], not_out, &[High, Low]);
        check(&mut g, "NAND", &[a, b], nand_out, &[High, High, High, Low]);
        check(&mut g, "AND", &[a, b], and_out, &[Low, Low, Low, High]);
        check(&mut g, "NOR", &[a, b], nor_out, &[High, Low, Low, Low]);
        check(&mut g, "XOR", &[a, b], xor_out, &[Low, High, High, Low]);
        check(&mut g, "XNOR", &[a, b], xnor_out, &[High, Low, Low, High]);
        check(&mut g, "OR", &[a, b], or_out, &[Low, High, High, High]);
        check(&mut g, "OR_NAND", &[a, b], or_nand_out, &[Low, High, High, High]);
    }

    #[test]
    fn test_sr_latch_metastable_release() {
        let mut g = SignalGraph::new();
        let mut s = Schematic::new();
        let set = g.add_signal(Some("SET"));
        let reset = g.add_signal(Some("RESET"));
        let latch = s.sr_latch(&mut g, set, reset);
        let q = latch.outputs[0];

        // forbidden state settles: both outputs forced HIGH
        g.drive(set, Some(Low));
        g.drive(reset, Some(Low));
        g.propagate();
        assert!(g.settled, "forbidden state is still a fixed point");
        assert_eq!(g.signals[q.0], High);

        // releasing both at once is the textbook race: the perfectly symmetric
        // latch oscillates, and propagate() reports it instead of looping forever
        g.drive(set, Some(High));
        g.drive(reset, Some(High));
        g.propagate();
        assert!(!g.settled, "symmetric release should not settle");
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
        check(&mut g,
              "FA_SUM",
              &[a, b, cin],
              sum,
              &[Low, High, High, Low, High, Low, Low, High]);
        check(&mut g,
              "FA_COUT",
              &[a, b, cin],
              cout,
              &[Low, Low, Low, High, Low, High, High, High]);
    }

    #[test]
    fn test_adder_4bit() {
        let mut g = SignalGraph::new();
        let mut s = Schematic::new();
        let a = [g.add_signal(Some("A0")),
                 g.add_signal(Some("A1")),
                 g.add_signal(Some("A2")),
                 g.add_signal(Some("A3"))];
        let b = [g.add_signal(Some("B0")),
                 g.add_signal(Some("B1")),
                 g.add_signal(Some("B2")),
                 g.add_signal(Some("B3"))];
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
    fn test_mux_2to1() {
        let mut g = SignalGraph::new();
        let mut s = Schematic::new();
        let a = g.add_signal(Some("A"));
        let b = g.add_signal(Some("B"));
        let sel = g.add_signal(Some("SEL"));
        let mux = s.mux_2to1(&mut g, a, b, sel);
        let out = mux.outputs[0];
        // sel=Low → output follows a
        g.drive(sel, Some(Low));
        check(&mut g, "MUX_SEL_LOW", &[a, b], out, &[Low, Low, High, High]);
        // sel=High → output follows b
        g.drive(sel, Some(High));
        check(&mut g,
              "MUX_SEL_HIGH",
              &[a, b],
              out,
              &[Low, High, Low, High]);
    }

    #[test]
    fn test_mux_4to1() {
        let mut g = SignalGraph::new();
        let mut s = Schematic::new();

        let a = g.add_signal(Some("A"));
        let b = g.add_signal(Some("B"));
        let c = g.add_signal(Some("C"));
        let d = g.add_signal(Some("D"));
        let s0 = g.add_signal(Some("S0")); // Low bit
        let s1 = g.add_signal(Some("S1")); // High bit

        let mux = s.mux_4to1(&mut g, a, b, c, d, s0, s1);
        let out = mux.outputs[0];

        // Case 00: Output follows A
        g.drive(s1, Some(Low));
        g.drive(s0, Some(Low));
        g.drive(b, Some(Low));
        g.drive(c, Some(Low));
        g.drive(d, Some(Low));
        check(&mut g, "MUX4_A", &[a], out, &[Low, High]);

        // Case 01: Output follows B
        g.drive(s1, Some(Low));
        g.drive(s0, Some(High));
        g.drive(a, Some(Low));
        g.drive(c, Some(Low));
        g.drive(d, Some(Low));
        check(&mut g, "MUX4_B", &[b], out, &[Low, High]);

        // Case 10: Output follows C
        g.drive(s1, Some(High));
        g.drive(s0, Some(Low));
        g.drive(a, Some(Low));
        g.drive(b, Some(Low));
        g.drive(d, Some(Low));
        check(&mut g, "MUX4_C", &[c], out, &[Low, High]);

        // Case 11: Output follows D
        g.drive(s1, Some(High));
        g.drive(s0, Some(High));
        g.drive(a, Some(Low));
        g.drive(b, Some(Low));
        g.drive(c, Some(Low));
        check(&mut g, "MUX4_D", &[d], out, &[Low, High]);
    }

    #[test]
    fn test_sr_latch() {
        let mut g = SignalGraph::new();
        let mut s = Schematic::new();
        let set = g.add_signal(Some("SET"));
        let reset = g.add_signal(Some("RESET"));
        let latch = s.sr_latch(&mut g, set, reset);
        let q = latch.outputs[0];
        let nq = latch.outputs[1];

        // POWER-ON: pulse SET to force known initial state
        g.drive(set, Some(Low));
        g.drive(reset, Some(High));
        g.propagate();
        assert_eq!(g.signals[q.0], High, "SET: Q should be HIGH");
        assert_eq!(g.signals[nq.0], Low, "SET: !Q should be LOW");

        // HOLD after SET
        g.drive(set, Some(High));
        g.drive(reset, Some(High));
        g.propagate();
        assert_eq!(g.signals[q.0], High, "HOLD: Q should stay HIGH");
        assert_eq!(g.signals[nq.0], Low, "HOLD: !Q should stay LOW");

        // RESET
        g.drive(set, Some(High));
        g.drive(reset, Some(Low));
        g.propagate();
        assert_eq!(g.signals[q.0], Low, "RESET: Q should be LOW");
        assert_eq!(g.signals[nq.0], High, "RESET: !Q should be HIGH");

        // HOLD after RESET
        g.drive(set, Some(High));
        g.drive(reset, Some(High));
        g.propagate();
        assert_eq!(g.signals[q.0], Low, "HOLD after RESET: Q should stay LOW");
        assert_eq!(g.signals[nq.0], High,
                   "HOLD after RESET: !Q should stay HIGH");

        // SET again
        g.drive(set, Some(Low));
        g.drive(reset, Some(High));
        g.propagate();
        assert_eq!(g.signals[q.0], High, "SET again: Q should be HIGH");
        assert_eq!(g.signals[nq.0], Low, "SET again: !Q should be LOW");

        // FORBIDDEN: both LOW → both outputs HIGH (undefined but predictable)
        g.drive(set, Some(Low));
        g.drive(reset, Some(Low));
        g.propagate();
        assert_eq!(g.signals[q.0], High, "Forbidden: Q should be HIGH");
        assert_eq!(g.signals[nq.0], High, "Forbidden: !Q should be HIGH");

        // RECOVERY from forbidden: genuinely metastable in real hardware
        // re-assert known state via SET pulse instead of releasing symmetrically
        g.drive(set, Some(Low));
        g.drive(reset, Some(High));
        g.propagate();
        assert_eq!(g.signals[q.0], High, "Recovery via SET: Q should be HIGH");
        assert_eq!(g.signals[nq.0], Low, "Recovery via SET: !Q should be LOW");

        // Stress test: alternate SET/RESET 10 times
        for _ in 0..10 {
            g.drive(set, Some(Low));
            g.drive(reset, Some(High));
            g.propagate();
            assert_eq!(g.signals[q.0], High, "Stress SET: Q");
            assert_eq!(g.signals[nq.0], Low, "Stress SET: !Q");

            g.drive(set, Some(High));
            g.drive(reset, Some(High));
            g.propagate();
            assert_eq!(g.signals[q.0], High, "Stress HOLD: Q");

            g.drive(set, Some(High));
            g.drive(reset, Some(Low));
            g.propagate();
            assert_eq!(g.signals[q.0], Low, "Stress RESET: Q");
            assert_eq!(g.signals[nq.0], High, "Stress RESET: !Q");

            g.drive(set, Some(High));
            g.drive(reset, Some(High));
            g.propagate();
            assert_eq!(g.signals[q.0], Low, "Stress HOLD after RESET: Q");
        }
    }
}
