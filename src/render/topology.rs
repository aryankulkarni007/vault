use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::text::Span;
use ratatui::widgets::Block;
use ratatui::widgets::Borders;
use ratatui::widgets::Widget;

use crate::render::layout::find_clusters;
use crate::sim::GateDescriptor;
use crate::sim::SignalGraph;
use crate::sim::SignalId;
use crate::sim::SignalState;
use crate::sim::TransistorKind;

/* this file is ai as fuck - drawing logic is hard af */

const TRANSISTOR_W: u16 = 4; // "[P] " or "[N] "
const CELL_W: u16 = 12; // width per PMOS column
const GATE_WIRE_W: u16 = 6; // space for gate label + wire "A ━━━━┤"
const ROW_H: u16 = 2; // rows per transistor (node + wire below)

const PCB_GREEN_DARK: Color = Color::Rgb(20, 60, 20); // Solder mask
const PCB_GREEN_SILK: Color = Color::Rgb(40, 100, 40); // Traces (inactive)
const ACCENT_GOLD: Color = Color::Rgb(255, 220, 80); // Exposed Copper/Gold

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

#[derive(Clone)]
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
    pub clusters: Vec<ClusterLayout>,
    pub output_signal: SignalId,
}

pub struct ClusterLayout {
    pub output: SignalId,
    pub pmos: Vec<TransistorNode>,
    pub nmos: Vec<TransistorNode>,
}

pub struct TopologyWidget<'a> {
    pub graph: &'a SignalGraph,
    pub layout: &'a GateLayout,
}

impl<'a> Widget for TopologyWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let output_state = self.graph.signals[self.layout.output_signal.0];
        let gold_style = Style::default().fg(ACCENT_GOLD);
        let silk_style = Style::default().fg(PCB_GREEN_SILK);

        // 1. DYNAMIC FRAME COLOR
        let border_color = if output_state == SignalState::High {
            ACCENT_GOLD
        } else {
            PCB_GREEN_DARK
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color))
            .title(Line::from(vec![
                Span::styled(" SEM_SCAN // ", silk_style),
                Span::styled(self.layout.label.to_uppercase(), gold_style.bold()),
            ]));

        let inner = block.inner(area);
        block.render(area, buf);

        if inner.width < 10 || inner.height < 10 {
            return;
        }

        // 2. CENTERING CALCULATIONS
        let cluster_padding = 4;
        let cluster_widths: Vec<u16> = self
            .layout
            .clusters
            .iter()
            .map(|c| (c.pmos.len() as u16 * 8).max(12))
            .collect();

        let total_w: u16 = cluster_widths.iter().sum::<u16>()
            + (cluster_widths.len().saturating_sub(1) as u16 * cluster_padding);
        let total_h = 9; // Rails + PMOS + Bridge + NMOS stack space

        let start_x = inner.x + (inner.width.saturating_sub(total_w) / 2);
        let start_y = inner.y + (inner.height.saturating_sub(total_h) / 2);

        // 3. RENDER CLUSTERS
        let mut x_cursor = start_x;
        for (idx, cluster) in self.layout.clusters.iter().enumerate() {
            let width = cluster_widths[idx];

            // Only render if within the frame's horizontal bounds
            if x_cursor < inner.right() {
                self.render_microscope_cell(buf, cluster, x_cursor, start_y, inner, width);
            }
            x_cursor += width + cluster_padding;
        }
    }
}

impl<'a> TopologyWidget<'a> {
    fn render_microscope_cell(
        &self,
        buf: &mut Buffer,
        cluster: &ClusterLayout,
        origin_x: u16,
        origin_y: u16,
        limit: Rect,
        width: u16,
    ) {
        // Local drawing helper with boundary clipping
        let mut draw = |rx: u16, ry: u16, s: &str, style: Style| {
            let tx = origin_x + rx;
            let ty = origin_y + ry;
            if tx >= limit.left() && tx < limit.right() && ty >= limit.top() && ty < limit.bottom()
            {
                buf.set_string(tx, ty, s, style);
            }
        };

        let vdd_s = wire_style(self.graph.signals[0]);
        let gnd_s = wire_style(self.graph.signals[1]);
        let out_s = wire_style(self.graph.signals[cluster.output.0]);

        // 1. DYNAMIC BOUNDARIES
        // Instead of a fixed width, we find where the transistors actually start and end.
        let p_count = cluster.pmos.len() as u16;
        let b_start = 2; // Fixed indent for the first transistor
        let b_end = ((p_count.saturating_sub(1)) * 8) + 2; // Position of the last transistor

        let substrate_style = Style::default().fg(Color::Rgb(15, 35, 15)); // Deep, dark moss green
        for ry in 0..=8 {
            for rx in b_start..=b_end {
                // Using a braille pattern for high-density "noise" texture
                draw(rx, ry, "⣿", substrate_style);
            }
        }

        // --- 2. POWER RAILS (Clipped to Transistors) ---
        for x in b_start..=b_end {
            if x == b_start {
                draw(x, 0, "┏", vdd_s);
                draw(x, 8, "┗", gnd_s);
            } else if x == b_end {
                draw(x, 0, "┓", vdd_s);
                draw(x, 8, "┛", gnd_s);
            } else {
                draw(x, 0, "━", vdd_s);
                draw(x, 8, "━", gnd_s);
            }
        }

        // --- 2. PMOS ARRAY (Pull-up) ---
        for (i, pmos) in cluster.pmos.iter().enumerate() {
            let px = (i as u16 * 8) + 2;
            let gate_s = wire_style(self.graph.signals[pmos.gate_signal.0]);
            let name = self.graph.signal_names[pmos.gate_signal.0]
                .as_deref()
                .unwrap_or("?");

            draw(px, 1, "┃", vdd_s);
            draw(px - 1, 2, name, gate_s);
            draw(px, 2, "║", gate_s); // Polysilicon Gate
            draw(px + 1, 2, "P", out_s);
            draw(px, 3, "┃", out_s);
        }

        // --- THE BRIDGE (Output Node) ---
        let bridge_y = 4;
        let b_start = 2;
        let b_end = ((cluster.pmos.len() as u16).saturating_sub(1) * 8) + 2;

        // Draw the horizontal line
        for bx in b_start..=b_end {
            draw(bx, bridge_y, "━", out_s);
        }

        // Draw the PMOS-to-Bridge connections
        for i in 0..cluster.pmos.len() as u16 {
            let px = (i * 8) + 2;
            if px == b_start {
                draw(px, bridge_y, "┗", out_s); // Top-left corner
            } else if px == b_end {
                draw(px, bridge_y, "┛", out_s); // Top-right corner
            } else {
                draw(px, bridge_y, "┻", out_s); // T-junction for middle PMOS
            }
        }

        // --- 4. NMOS STACK (Pull-down) ---
        let nx = (b_start + b_end) / 2;
        // Bridge to NMOS junction
        draw(
            nx,
            bridge_y,
            if cluster.pmos.len() > 1 { "┻" } else { "┃" },
            out_s,
        );

        for (i, nmos) in cluster.nmos.iter().enumerate() {
            let ny = 5 + (i as u16 * 2);
            let gate_s = wire_style(self.graph.signals[nmos.gate_signal.0]);
            let name = self.graph.signal_names[nmos.gate_signal.0]
                .as_deref()
                .unwrap_or("?");

            draw(nx - 1, ny, name, gate_s);
            draw(nx, ny, "║", gate_s);
            draw(nx + 1, ny, "N", gate_s);

            if i < cluster.nmos.len() - 1 {
                let mid_s = wire_style(self.graph.signals[nmos.vertical_out.0]);
                draw(nx, ny + 1, "┃", mid_s);
            } else {
                // Ground Connection with correct "Bottom T" junction
                draw(nx, ny + 1, "┃", gnd_s);
                draw(nx, 8, "┻", gnd_s);
            }
        }
    }
}

fn layout_cluster(graph: &SignalGraph, transistors: &[usize]) -> ClusterLayout {
    let (pmos_ids, nmos_ids): (Vec<usize>, Vec<usize>) = transistors
        .iter()
        .partition(|&&id| graph.kinds[id] == TransistorKind::PMOS);

    let output = pmos_ids
        .first()
        .map(|&id| graph.drains[id])
        .unwrap_or(SignalId(1));

    let mut nmos_ordered: Vec<usize> = Vec::new();
    let mut current_signal = output;
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

    let pmos_nodes = pmos_ids
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
        });

    let nmos_nodes = nmos_ordered
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
        });

    ClusterLayout {
        output,
        pmos: pmos_nodes.collect(),
        nmos: nmos_nodes.collect(),
    }
}

pub fn layout_gate(graph: &SignalGraph, descriptor: &GateDescriptor, label: &str) -> GateLayout {
    let clusters = find_clusters(graph, &descriptor.transistors);
    let cluster_layouts: Vec<ClusterLayout> =
        clusters.iter().map(|c| layout_cluster(graph, c)).collect();
    GateLayout {
        label: label.to_string(),
        clusters: cluster_layouts,
        output_signal: descriptor.output,
    }
}
