use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::style::Style;
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
        let output_state = self.graph.signals[self.layout.output_signal.0];
        let border_style = wire_style(output_state);

        let block = Block::default()
            .title(format!("─ {} ", self.layout.label))
            .borders(Borders::ALL)
            .border_style(border_style);

        let inner = block.inner(area);
        block.render(area, buf);

        let vdd_state = self.graph.signals[0];
        let vdd_style = wire_style(vdd_state);
        let vdd_char = wire_char(vdd_state);

        let label = " VDD ";
        let rail_w = (inner.width.saturating_sub(label.len() as u16)) / 2;
        let rail: String = vdd_char.repeat(rail_w as usize);
        let vdd_line = format!("{}{}{}", rail, label, rail);
        buf.set_string(inner.x, inner.y, &vdd_line, vdd_style);

        let pmos_row_y = inner.y + 2; // one line gap after VDD rail

        for node in self
            .layout
            .nodes
            .iter()
            .filter(|n| n.kind == TransistorKind::PMOS)
        {
            let x = inner.x + GATE_WIRE_W + node.col * CELL_W;

            // gate wire: "A ━━━━┤"
            let gate_state = self.graph.signals[node.gate_signal.0];
            let gate_name = self.graph.signal_names[node.gate_signal.0]
                .as_deref()
                .unwrap_or("?");
            let gate_wire = format!("{} ━━━┤", gate_name);
            buf.set_string(
                x - GATE_WIRE_W,
                pmos_row_y,
                &gate_wire,
                wire_style(gate_state),
            );

            // transistor block
            let conducting_state = self.graph.signals[node.vertical_out.0];
            buf.set_string(x, pmos_row_y, "[P]", wire_style(conducting_state));

            // vertical wire below
            let vert_state = self.graph.signals[node.vertical_out.0];
            buf.set_string(x + 1, pmos_row_y + 1, "┃", wire_style(vert_state));

            // merge line — connects PMOS outputs down to OUT
            let pmos_count = self
                .layout
                .nodes
                .iter()
                .filter(|n| n.kind == TransistorKind::PMOS)
                .count() as u16;
            let merge_y = pmos_row_y + 2;
            let out_state = self.graph.signals[self.layout.output_signal.0];
            let out_style = wire_style(out_state);

            // draw horizontal merge bar
            let merge_x_start = inner.x + GATE_WIRE_W + 1;
            let merge_x_end = inner.x + GATE_WIRE_W + (pmos_count - 1) * CELL_W + 1;
            for x in merge_x_start..=merge_x_end {
                buf.set_string(x, merge_y, "─", out_style);
            }

            // corners and center drop
            buf.set_string(merge_x_start, merge_y, "└", out_style);
            buf.set_string(merge_x_end, merge_y, "┘", out_style);
            let mid_x = (merge_x_start + merge_x_end) / 2;
            buf.set_string(mid_x, merge_y, "┬", out_style);
            buf.set_string(mid_x, merge_y + 1, "┃", out_style);

            // OUT label
            let out_label = "┃ OUT".to_string();
            buf.set_string(mid_x, merge_y + 1, &out_label, out_style);

            let nmos_start_y = merge_y + 2;
            for node in self
                .layout
                .nodes
                .iter()
                .filter(|n| n.kind == TransistorKind::NMOS)
            {
                let y = nmos_start_y + node.row * ROW_H;
                let gate_state = self.graph.signals[node.gate_signal.0];
                let gate_name = self.graph.signal_names[node.gate_signal.0]
                    .as_deref()
                    .unwrap_or("?");

                // gate wire
                let gate_wire = format!("{} ╌╌╌╌┤", gate_name);
                buf.set_string(mid_x - GATE_WIRE_W, y, &gate_wire, wire_style(gate_state));

                // transistor block
                buf.set_string(mid_x, y, "[N]", wire_style(gate_state));

                // vertical wire below
                let vert_state = self.graph.signals[node.vertical_out.0];
                buf.set_string(mid_x, y + 1, "┃", wire_style(vert_state));

                let gnd_state = self.graph.signals[1];
                let gnd_char = wire_char(gnd_state);

                let label = " GND ";
                let rail_w = (inner.width.saturating_sub(label.len() as u16)) / 2;
                let rail: String = gnd_char.repeat(rail_w as usize);
                let gnd_line = format!("{}{}{}", rail, label, rail);
                buf.set_string(
                    inner.x,
                    inner.y + inner.height - 1,
                    &gnd_line,
                    wire_style(gnd_state),
                );
            }
        }
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
