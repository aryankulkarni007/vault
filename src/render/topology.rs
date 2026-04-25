use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Widget};

use crate::SignalGraph;
use crate::SignalState;
use crate::sim::transistor::{GateDescriptor, SignalId, TransistorKind};

// ┌─────────────────────────────────────────────────────────────┐
// │  GLASS PCB PALETTE                                          │
// │                                                             │
// │  The chip is viewed through a glass substrate. The dark     │
// │  background is the void below the silicon. Gold traces are  │
// │  suspended in the glass. Three silicon texture densities    │
// │  convey doping concentration.                               │
// └─────────────────────────────────────────────────────────────┘

const SILICON_ACTIVE: Color = Color::Rgb(15, 35, 15); // Dense braille in transistor regions
const SILICON_FIELD: Color = Color::Rgb(10, 25, 12); // Patchy braille between clusters
const SILICON_TRACE: Color = Color::Rgb(8, 20, 10); // Light braille under metal traces
const ACCENT_GOLD: Color = Color::Rgb(255, 220, 80); // Active gold
const GOLD_DIM: Color = Color::Rgb(138, 112, 48); // Inactive/delimiter gold
const GOLD_BRIGHT: Color = Color::Rgb(255, 235, 120); // HIGH signal glow
const SILK_ETCH: Color = Color::Rgb(96, 136, 96); // Silkscreen text etched in glass
const CONFLICT_RED: Color = Color::Rgb(255, 50, 50);

fn wire_style(state: SignalState) -> Style {
    match state {
        SignalState::High => Style::default().fg(GOLD_BRIGHT),
        SignalState::Low => Style::default().fg(GOLD_DIM),
        SignalState::Floating => Style::default().fg(GOLD_DIM),
        SignalState::Conflict => Style::default().fg(CONFLICT_RED),
    }
}

// Texture characters cycling through variations for natural silicon look
const ACTIVE_TEX: &[&str] = &["⣿", "⣿", "⣿", "⣿", "⣼", "⣿", "⣿", "⣿"];
const FIELD_TEX: &[&str] = &["⣭", "⣯", "⣻", "⣭", "⣯", "⣻", "⣭", "⣯"];
const TRACE_TEX: &[&str] = &["⢽", "⢽", "⢿", "⢽", "⢽", "⢿", "⢽", "⢽"];

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
        let silk_style = Style::default().fg(SILK_ETCH);

        let border_color = if output_state == SignalState::High {
            ACCENT_GOLD
        } else {
            GOLD_DIM
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color))
            .title(Line::from(vec![
                Span::styled(" SEMSCAN // ", silk_style),
                Span::styled(self.layout.label.to_uppercase(), gold_style.bold()),
            ]));

        let inner = block.inner(area);
        block.render(area, buf);

        if inner.width < 10 || inner.height < 10 {
            return;
        }

        // ── Layout calculations ──────────────────────────────────
        //
        //  Vertical structure per cluster:
        //    row 0:   VDD rail taps (┳)
        //    row 1-3: PMOS region
        //    row 4:   Bridge (PMOS/NMOS boundary)
        //    row 5:   Trench (routing channel)
        //    row 6+:  NMOS region
        //    last:    GND rail
        //
        //  Between clusters: delimiter column (│) with field texture
        // ──────────────────────────────────────────────────────────

        let delimiter_w = 1u16;
        let cluster_widths: Vec<u16> = self
            .layout
            .clusters
            .iter()
            .map(|c| {
                let p_count = c.pmos.len() as u16;
                // body_right = 2 + (p_count-1)*8, add 3 for right padding
                let body_right = 2 + (p_count.saturating_sub(1)) * 8;
                body_right + 3 // left pad 2 + body + right pad 1 = body_right + 3
            })
            .collect();

        let num_clusters = cluster_widths.len();
        let total_w: u16 = cluster_widths.iter().sum::<u16>()
            + (num_clusters.saturating_sub(1) as u16 * delimiter_w);

        let max_nmos_rows = self
            .layout
            .clusters
            .iter()
            .map(|c| c.nmos.len() as u16 * 2)
            .max()
            .unwrap_or(2);
        let total_h = 1 + 3 + 1 + 1 + 1 + max_nmos_rows + 1;
        //             VDD  PMOS bridge trench NMOS     GND

        let start_x = inner.x + (inner.width.saturating_sub(total_w) / 2);
        let start_y = inner.y + (inner.height.saturating_sub(total_h) / 2);

        // ── Compute cluster X positions ──────────────────────────
        let mut cluster_x: Vec<u16> = Vec::with_capacity(num_clusters);
        let mut cx = start_x;
        for (ci, &cw) in cluster_widths.iter().enumerate() {
            cluster_x.push(cx);
            cx += cw;
            if ci < num_clusters - 1 {
                cx += delimiter_w;
            }
        }

        // ── Step 1: Draw continuous silicon texture ──────────────
        self.render_substrate(buf, start_x, start_y, total_w, total_h, &cluster_widths);

        // ── Step 2: Draw shared VDD rail ─────────────────────────
        let vdd_y = start_y;
        let vdd_s = wire_style(self.graph.signals[0]);
        for dx in 0..total_w {
            let ch = "━";
            buf.set_string(start_x + dx, vdd_y, ch, vdd_s);
        }

        // ── Step 3: Draw shared GND rail ─────────────────────────
        let gnd_y = start_y + total_h - 1;
        let gnd_s = wire_style(self.graph.signals[1]);
        for dx in 0..total_w {
            buf.set_string(start_x + dx, gnd_y, "━", gnd_s);
        }

        // ── Step 4: Draw cluster delimiters (vertical bars) ──────
        let delim_s = Style::default().fg(GOLD_DIM);
        for ci in 0..num_clusters.saturating_sub(1) {
            let dx = cluster_x[ci] + cluster_widths[ci];
            let trench_row = 5u16;
            for dy in 1..total_h - 1 {
                let ch = if dy == trench_row { "╪" } else { "│" };
                buf.set_string(dx, start_y + dy, ch, delim_s);
            }
        }

        // ── Step 5: Draw each cluster ────────────────────────────
        for (ci, cluster) in self.layout.clusters.iter().enumerate() {
            self.render_cluster(
                buf,
                cluster,
                cluster_x[ci],
                start_y,
                cluster_widths[ci],
                total_h,
            );
        }

        // ── Step 6: Draw trench traces (on top of texture) ───────
        self.render_trench(buf, start_x, start_y, total_w, &cluster_widths, &cluster_x);
    }
}

impl<'a> TopologyWidget<'a> {
    /// Fills the entire view with continuous silicon texture.
    /// Three zones: ACTIVE (transistor areas), TRACE (trench row), FIELD (delimiters).
    fn render_substrate(
        &self,
        buf: &mut Buffer,
        origin_x: u16,
        origin_y: u16,
        total_w: u16,
        total_h: u16,
        cluster_widths: &[u16],
    ) {
        let delimiter_w = 1u16;

        for dy in 1..total_h - 1 {
            // Skip VDD (row 0) and GND (last row)
            let is_trench = dy == 5;
            let is_bridge = dy == 4;
            let mut cx = origin_x;

            for (ci, &cw) in cluster_widths.iter().enumerate() {
                let color = if is_trench {
                    SILICON_TRACE
                } else if is_bridge {
                    SILICON_FIELD // bridge row gets field texture, traces go on top
                } else {
                    SILICON_ACTIVE
                };
                let tex: &[&str] = if is_trench {
                    TRACE_TEX
                } else if is_bridge {
                    FIELD_TEX
                } else {
                    ACTIVE_TEX
                };
                let style = Style::default().fg(color);

                for dx in 0..cw {
                    let idx = ((cx - origin_x + dx) as usize + dy as usize * 7) % tex.len();
                    buf.set_string(cx + dx, origin_y + dy, tex[idx], style);
                }
                cx += cw;

                // delimiter column
                if ci < cluster_widths.len() - 1 {
                    let fstyle = Style::default().fg(SILICON_FIELD);
                    let fidx = ((cx - origin_x) as usize + dy as usize * 7) % FIELD_TEX.len();
                    buf.set_string(cx, origin_y + dy, FIELD_TEX[fidx], fstyle);
                    cx += delimiter_w;
                }
            }
        }
    }

    /// Draws a single CMOS cluster.
    ///
    ///  ┌──────────────────────────────────┐
    ///  │  ┳━━━━━━━┳        VDD taps       │
    ///  │  ┃⣿⣿⣿⣿⣿⣿⣿┃   PMOS body    │
    ///  │ A║P⣿⣿⣿B║P        gate bars     │
    ///  │  ┃⣿⣿⣿⣿⣿⣿⣿┃               │
    ///  │  ┗━━━┳━━━┛        bridge        │
    ///  │      ┃             output drop   │
    ///  │  ⣿⣿⣿A║N⣿⣿        NMOS gates    │
    ///  │  ⣿⣿⣿┃⣿⣿⣿⣿        series link   │
    ///  │  ⣿⣿⣿B║N⣿⣿                    │
    ///  │  ━━━━┻━━━━        GND connect   │
    ///  └──────────────────────────────────┘
    fn render_cluster(
        &self,
        buf: &mut Buffer,
        cluster: &ClusterLayout,
        ox: u16,
        oy: u16,
        _width: u16,
        total_h: u16,
    ) {
        let vdd_s = wire_style(self.graph.signals[0]);
        let gnd_s = wire_style(self.graph.signals[1]);
        let out_s = wire_style(self.graph.signals[cluster.output.0]);

        let p_count = cluster.pmos.len() as u16;
        let n_count = cluster.nmos.len() as u16;

        // PMOS body: left border at col 2, each transistor occupies 8 cols
        // Gate center for PMOS i is at col (2 + i*8)
        let body_left = 2u16;
        let body_right = body_left + (p_count.saturating_sub(1)) * 8;
        // NMOS stack sits at the horizontal center of the PMOS body
        let nx = (body_left + body_right) / 2;

        // ── VDD taps (row 0) ─────────────────────────────────
        for i in 0..p_count {
            let px = body_left + i * 8;
            buf.set_string(ox + px, oy, "┳", vdd_s);
            buf.set_string(ox + px, oy + 1, "┃", vdd_s);
        }

        // ── PMOS body: rows 1-3 ──────────────────────────────
        // row 1: top vertical connections (no longer draws "━" to prevent box artifacts)
        for i in 0..p_count {
            let px = body_left + i * 8;
            buf.set_string(ox + px, oy + 1, "┃", vdd_s);
        }

        // row 2: transistors
        for i in 0..p_count {
            let px = body_left + i * 8; // gate center column
            let gate_s = wire_style(self.graph.signals[cluster.pmos[i as usize].gate_signal.0]);
            let name = self.graph.signal_names[cluster.pmos[i as usize].gate_signal.0]
                .as_deref()
                .unwrap_or("?");
            buf.set_string(ox + px - 1, oy + 2, name, gate_s);
            buf.set_string(ox + px, oy + 2, "║", gate_s);
            buf.set_string(ox + px + 1, oy + 2, "P", out_s);
        }

        // Row 3: bottom vertical connections
        for i in 0..p_count {
            let px = body_left + i * 8;
            buf.set_string(ox + px, oy + 3, "┃", out_s);
        }

        // ── Bridge (row 4) ───────────────────────────────────
        for bx in body_left..=body_right {
            buf.set_string(ox + bx, oy + 4, "━", out_s);
        }
        for i in 0..p_count {
            let px = body_left + i * 8;
            let ch = if p_count == 1 {
                "┃"
            } else if i == 0 {
                "┗"
            } else if i == p_count - 1 {
                "┛"
            } else {
                "┻"
            };
            buf.set_string(ox + px, oy + 4, ch, out_s);
        }
        // center junction for output pillar (overwrites the ━ at nx)
        if p_count > 1 {
            buf.set_string(ox + nx, oy + 4, "┳", out_s);
        }

        // ── output pillar into trench (row 5) ─────────────────
        buf.set_string(ox + nx, oy + 5, "┃", out_s);
        buf.set_string(ox + nx, oy + 6, "┃", out_s);

        // ── NMOS stack (rows 7 to 7 + n_count*2 - 1) ─────────
        for i in 0..n_count {
            let ny = oy + 7 + i * 2;
            let nmos = &cluster.nmos[i as usize];
            let gate_s = wire_style(self.graph.signals[nmos.gate_signal.0]);
            let name = self.graph.signal_names[nmos.gate_signal.0]
                .as_deref()
                .unwrap_or("?");

            buf.set_string(ox + nx - 1, ny, name, gate_s);
            buf.set_string(ox + nx, ny, "║", gate_s);
            buf.set_string(ox + nx + 1, ny, "N", gate_s);

            if i + 1 < n_count {
                // series connection down to next nmos
                let mid_s = wire_style(self.graph.signals[nmos.vertical_out.0]);
                buf.set_string(ox + nx, ny + 1, "┃", mid_s);
            } else {
                let gnd_rail_y = oy + total_h - 1;
                for dy in (ny + 1)..gnd_rail_y {
                    buf.set_string(ox + nx, dy, "┃", gnd_s);
                }
                buf.set_string(ox + nx, gnd_rail_y, "┻", gnd_s);
            }
        }
    }
    /// Draws gold traces in the trench row (row 5), connecting
    /// output pillars between clusters with named signal lines.
    ///
    ///  ┃ n1 ═══════════╪ n2 ═══════╪ n3 ══════ Y
    fn render_trench(
        &self,
        buf: &mut Buffer,
        origin_x: u16,
        origin_y: u16,
        total_w: u16,
        cluster_widths: &[u16],
        cluster_x: &[u16],
    ) {
        let trench_y = origin_y + 5;

        for ci in 0..self.layout.clusters.len() {
            let cluster = &self.layout.clusters[ci];
            let cx = cluster_x[ci];
            let p_count = cluster.pmos.len() as u16;
            let body_left = 2u16;
            let body_right = body_left + (p_count.saturating_sub(1)) * 8;
            let nx = (body_left + body_right) / 2;
            let pillar_x = cx + nx;

            let sig_name = self.graph.signal_names[cluster.output.0]
                .as_deref()
                .unwrap_or("?");
            let out_s = wire_style(self.graph.signals[cluster.output.0]);

            // Ensure the vertical pillar is connected to the trench trace
            // This character acts as the "Via" between layers
            buf.set_string(pillar_x, trench_y, "┃", out_s);

            // Signal name after pillar for high-traceability debugging
            let name_x = pillar_x + 1;
            buf.set_string(name_x, trench_y, sig_name, Style::default().fg(GOLD_BRIGHT));

            // Calculate where the trace needs to go
            let trace_start = name_x + sig_name.len() as u16;
            let trace_end = if ci < self.layout.clusters.len() - 1 {
                // Connect to the NEXT cluster's pillar position
                let ncx = cluster_x[ci + 1];
                let ncl = &self.layout.clusters[ci + 1];
                let np_count = ncl.pmos.len() as u16;
                let n_body_right = 2 + (np_count.saturating_sub(1)) * 8;
                ncx + (2 + n_body_right) / 2
            } else {
                // If it's the final cluster, run the trace to the edge of the substrate
                origin_x + total_w - 1
            };

            // Draw the horizontal "Metal 2" style interconnect
            for tx in trace_start..trace_end {
                buf.set_string(tx, trench_y, "═", out_s);
            }

            // Final output label (e.g., "Y") at the very end of the line
            if ci == self.layout.clusters.len() - 1 {
                let final_name = self.graph.signal_names[self.layout.output_signal.0]
                    .as_deref()
                    .unwrap_or("Y");
                let label_x = trace_end.saturating_sub(final_name.len() as u16);
                if label_x > trace_start {
                    buf.set_string(
                        label_x,
                        trench_y,
                        final_name,
                        Style::default().fg(GOLD_BRIGHT).bold(),
                    );
                }
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

/// Recursively flattens a gate descriptor tree into individual CMOS clusters.
fn collect_clusters(graph: &SignalGraph, descriptor: &GateDescriptor) -> Vec<ClusterLayout> {
    match &descriptor.sub_gates {
        None => {
            let t = descriptor.transistors.as_ref().unwrap();
            vec![layout_cluster(graph, t)]
        }
        Some(sub_gates) => sub_gates
            .iter()
            .flat_map(|sg| collect_clusters(graph, sg))
            .collect(),
    }
}

pub fn layout_gate(graph: &SignalGraph, descriptor: &GateDescriptor, label: &str) -> GateLayout {
    GateLayout {
        label: label.to_string(),
        clusters: collect_clusters(graph, descriptor),
        output_signal: descriptor.output,
    }
}
