use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::text::Span;
use ratatui::widgets::Block;
use ratatui::widgets::Borders;
use ratatui::widgets::Widget;

use crate::sim::GateDescriptor;
use crate::sim::SignalGraph;
use crate::sim::SignalId;
use crate::sim::SignalState;
use crate::sim::TransistorKind;

const TRANSISTOR_W: u16 = 4; // "[P] " or "[N] "
const CELL_W: u16 = 12; // width per PMOS column
const GATE_WIRE_W: u16 = 6; // space for gate label + wire "A ━━━━┤"
const ROW_H: u16 = 2; // rows per transistor (node + wire below)

fn wire_style(state: SignalState) -> Style {
    match state {
        SignalState::High => Style::default().fg(Color::Rgb(255, 220, 80)), // bright gold, live
        SignalState::Low => Style::default().fg(Color::Rgb(20, 60, 20)),
        SignalState::Floating => Style::default().fg(Color::Rgb(20, 60, 20)),
        SignalState::Conflict => Style::default().fg(Color::Rgb(255, 50, 50)), // red
    }
}

fn wire_char(state: SignalState) -> &'static str {
    match state {
        SignalState::High => "━",
        SignalState::Low => "╌",
        SignalState::Floating => "─",
        SignalState::Conflict => "▓",
    }
}

pub struct TransistorNode {
    pub id: usize,
    pub col: u16,
    pub row: u16,
    pub gate_signal: SignalId,
    pub vertical_in: SignalId,
    pub vertical_out: SignalId,
    pub kind: TransistorKind,
}

pub struct GateLayout {
    pub label: String,
    pub nodes: Vec<TransistorNode>,
    pub output_signal: SignalId,
}

pub struct TopologyWidget<'a> {
    pub graph: &'a SignalGraph,
    pub layout: &'a GateLayout,
}

impl<'a> Widget for TopologyWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // 1. STATE & STYLING PREP
        let output_state = self.graph.signals[self.layout.output_signal.0];
        let border_color = wire_style(output_state);
        let title_style = Style::default().fg(Color::Rgb(255, 200, 0));

        // 2. BLOCK RENDER
        // We use a Line to mix styles: border characters inherit block style, text is gold.
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(border_color)
            .title(Line::from(vec![
                Span::raw("─ "),
                Span::styled(self.layout.label.to_uppercase(), title_style),
                Span::raw(" "),
            ]));

        // Calculate the inner area (where we can actually draw)
        let inner = block.inner(area);
        block.render(area, buf);

        // If the window is too tiny to even show the inner area, abort to prevent panics.
        if inner.width == 0 || inner.height == 0 {
            return;
        }

        // 3. DIMENSION CALCULATION
        // Partition nodes once to avoid multiple filter/iter passes
        let (pmos_nodes, nmos_nodes): (Vec<_>, Vec<_>) = self
            .layout
            .nodes
            .iter()
            .partition(|n| n.kind == TransistorKind::PMOS);

        let p_count = pmos_nodes.len() as u16;
        let n_count = nmos_nodes.len() as u16;

        // Content Width: Space for gate labels + width of all PMOS columns
        let content_w = GATE_WIRE_W + (p_count.max(1) * CELL_W);
        // Content Height: VDD + Gap + PMOS + Merge + (NMOS stack) + GND
        let content_h = 4 + (n_count * ROW_H) + 1;

        // 4. CENTERING & CLIPPING LOGIC
        // We calculate offsets relative to 'inner' area.
        let offset_x = inner.x + inner.width.saturating_sub(content_w) / 2;
        let offset_y = inner.y + inner.height.saturating_sub(content_h) / 2;

        // Boundary check helper: prevents writing outside the widget's allocated area.
        let mut draw = |x_rel: u16, y_rel: u16, text: &str, style: Style| {
            let abs_x = offset_x + x_rel;
            let abs_y = offset_y + y_rel;
            if abs_x < inner.right() && abs_y < inner.bottom() {
                buf.set_string(abs_x, abs_y, text, style);
            }
        };

        // 5. DRAW VDD RAIL
        let vdd_state = self.graph.signals[0];
        let vdd_s = wire_style(vdd_state);
        let v_char = wire_char(vdd_state);
        let vdd_label = " VDD ";
        let v_rail_len = (content_w.saturating_sub(vdd_label.len() as u16)) / 2;
        let vdd_line = format!(
            "{}{}{}",
            v_char.repeat(v_rail_len as usize),
            vdd_label,
            v_char.repeat(v_rail_len as usize)
        );
        draw(0, 0, &vdd_line, vdd_s);

        // 6. DRAW PMOS ARRAY (Top)
        let pmos_y = 2;
        for node in &pmos_nodes {
            let x = GATE_WIRE_W + node.col * CELL_W;
            let gate_s = wire_style(self.graph.signals[node.gate_signal.0]);
            let out_s = wire_style(self.graph.signals[node.vertical_out.0]);
            let name = self.graph.signal_names[node.gate_signal.0]
                .as_deref()
                .unwrap_or("?");

            draw(
                x - GATE_WIRE_W,
                pmos_y,
                &format!("{:<2} ━━━┤", name),
                gate_s,
            );
            draw(x, pmos_y, "[P]", out_s);
            draw(x + 1, pmos_y + 1, "┃", out_s);
        }

        // 7. DRAW MERGE BAR & OUTPUT
        let merge_y = pmos_y + 2;
        let out_s = wire_style(output_state);
        let merge_start = GATE_WIRE_W + 1;
        let merge_end = GATE_WIRE_W + (p_count.saturating_sub(1) * CELL_W) + 1;

        for mx in merge_start..=merge_end {
            draw(mx, merge_y, "─", out_s);
        }
        draw(merge_start, merge_y, "└", out_s);
        draw(merge_end, merge_y, "┘", out_s);

        let mid_x = (merge_start + merge_end) / 2;
        draw(mid_x, merge_y, "┬", out_s);
        draw(mid_x, merge_y + 1, "┃ OUT", out_s);

        // 8. DRAW NMOS STACK (Bottom)
        let nmos_start_y = merge_y + 2;
        for (i, node) in nmos_nodes.iter().enumerate() {
            let y = nmos_start_y + (i as u16 * ROW_H);
            let gate_s = wire_style(self.graph.signals[node.gate_signal.0]);
            let name = self.graph.signal_names[node.gate_signal.0]
                .as_deref()
                .unwrap_or("?");

            draw(
                mid_x - GATE_WIRE_W,
                y,
                &format!("{:<2} ╌╌╌╌┤", name),
                gate_s,
            );
            draw(mid_x, y, "[N]", gate_s);

            // Only draw vertical connector if there's another transistor below
            if (i as u16) < n_count - 1 {
                let v_out_s = wire_style(self.graph.signals[node.vertical_out.0]);
                draw(mid_x, y + 1, "┃", v_out_s);
            }
        }

        // 9. DRAW GND RAIL
        let gnd_state = self.graph.signals[1];
        let gnd_s = wire_style(gnd_state);
        let g_char = wire_char(gnd_state);
        let gnd_label = " GND ";
        let g_rail_len = (content_w.saturating_sub(gnd_label.len() as u16)) / 2;
        let gnd_line = format!(
            "{}{}{}",
            g_char.repeat(g_rail_len as usize),
            gnd_label,
            g_char.repeat(g_rail_len as usize)
        );
        draw(0, content_h - 1, &gnd_line, gnd_s);
    }
}

pub fn layout_gate(graph: &SignalGraph, descriptor: &GateDescriptor, label: &str) -> GateLayout {
    let (pmos_ids, nmos_ids): (Vec<usize>, Vec<usize>) = descriptor
        .transistors
        .iter()
        .partition(|&&id| graph.kinds[id] == TransistorKind::PMOS);
    let mut nmos_ordered: Vec<usize> = Vec::new();
    let mut current_signal = descriptor.output;

    loop {
        let next = nmos_ids
            .iter()
            .find(|&&id| graph.drains[id] == current_signal);
        match next {
            Some(&id) => {
                nmos_ordered.push(id);
                current_signal = graph.sources[id];
            }
            None => break,
        }
    }
    let nodes: Vec<TransistorNode> = pmos_ids
        .iter()
        .enumerate()
        .map(|(col, &id)| TransistorNode {
            id,
            kind: TransistorKind::PMOS,
            gate_signal: graph.gates[id],
            vertical_in: graph.sources[id],
            vertical_out: graph.drains[id],
            col: col as u16,
            row: 0,
        })
        .chain(
            nmos_ordered
                .iter()
                .enumerate()
                .map(|(row, &id)| TransistorNode {
                    id,
                    kind: TransistorKind::NMOS,
                    gate_signal: graph.gates[id],
                    vertical_in: graph.drains[id],
                    vertical_out: graph.sources[id],
                    col: 0,
                    row: row as u16,
                }),
        )
        .collect();

    GateLayout {
        label: label.to_string(),
        nodes,
        output_signal: descriptor.output,
    }
}
