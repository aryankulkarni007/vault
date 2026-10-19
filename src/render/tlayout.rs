use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Widget};

use crate::sim::gate::{GateKind, UnitDescriptor};
use crate::sim::transistor::{
    GateBody, GateDescriptor, SignalGraph, SignalId, SignalState, TransistorKind,
};

// ┌─────────────────────────────────────────────────────────────┐
// │  GLASS PCB PALETTE                                          │
// │                                                             │
// │  The chip is viewed through a glass substrate. The dark     │
// │  background is the void below the silicon. Gold traces are  │
// │  suspended in the glass. Three silicon texture densities    │
// │  convey doping concentration.                               │
// └─────────────────────────────────────────────────────────────┘

const SILICON_ACTIVE: Color = Color::Rgb(15, 35, 15); // Dense braille in transistor regions
const SILICON_TRACE: Color = Color::Rgb(8, 20, 10); // Light braille under the output row
const ACCENT_GOLD: Color = Color::Rgb(255, 220, 80); // Active gold
const GOLD_DIM: Color = Color::Rgb(138, 112, 48); // LOW signal / delimiter gold
const GOLD_BRIGHT: Color = Color::Rgb(255, 235, 120); // HIGH signal glow
const SILK_ETCH: Color = Color::Rgb(96, 136, 96); // Silkscreen text etched in glass
const FLOAT_GREY: Color = Color::Rgb(70, 90, 70); // undriven, uncharged wire
const DEVICE_OFF: Color = Color::Rgb(60, 80, 60); // transistor switch open
const CONFLICT_RED: Color = Color::Rgb(255, 50, 50);

fn wire_style(state: SignalState) -> Style {
    match state {
        SignalState::High => Style::default().fg(GOLD_BRIGHT),
        SignalState::Low => Style::default().fg(GOLD_DIM),
        SignalState::Floating => Style::default().fg(FLOAT_GREY),
        SignalState::Conflict => Style::default().fg(CONFLICT_RED),
    }
}

// Texture characters cycling through variations for natural silicon look
const ACTIVE_TEX: &[&str] = &["⣿", "⣿", "⣿", "⣿", "⣼", "⣿", "⣿", "⣿"];
const TRACE_TEX: &[&str] = &["⢽", "⢽", "⢿", "⢽", "⢽", "⢿", "⢽", "⢽"];

/// Display name of a net: its given name, or `n<id>` for anonymous internal
/// nets, so every wire in the picture can be told apart.
pub fn net_name(graph: &SignalGraph, id: SignalId) -> String {
    graph.signal_names[id.0]
        .clone()
        .unwrap_or_else(|| format!("n{}", id.0))
}

// ── Series-parallel extraction ───────────────────────────────────
//
//  A static CMOS stage is two switch networks sharing one output:
//
//        VDD                    The pull-up (PMOS) runs VDD -> OUT,
//         │                     the pull-down (NMOS) runs OUT -> GND.
//     [pull-up]                 Each one is a series-parallel graph of
//         │                     transistors: NAND = parallel up, series
//        OUT                    down; NOR = the dual. Reducing the edge
//         │                     list to one tree lets any such stage be
//    [pull-down]                drawn the way it is wired.
//         │
//        GND
// ──────────────────────────────────────────────────────────────

/// A two-terminal switch network, oriented top to bottom.
#[derive(Clone, Debug)]
pub enum Network {
    Device {
        id: usize,
        gate: SignalId,
        top: SignalId,
        bottom: SignalId,
    },
    Series(Vec<Network>),
    Parallel(Vec<Network>),
}

impl Network {
    pub fn top(&self) -> SignalId {
        match self {
            Network::Device { top, .. } => *top,
            Network::Series(c) => c[0].top(),
            Network::Parallel(c) => c[0].top(),
        }
    }

    pub fn bottom(&self) -> SignalId {
        match self {
            Network::Device { bottom, .. } => *bottom,
            Network::Series(c) => c[c.len() - 1].bottom(),
            Network::Parallel(c) => c[0].bottom(),
        }
    }

    fn flip(self) -> Network {
        match self {
            Network::Device {
                id,
                gate,
                top,
                bottom,
            } => Network::Device {
                id,
                gate,
                top: bottom,
                bottom: top,
            },
            Network::Series(c) => Network::Series(c.into_iter().rev().map(Network::flip).collect()),
            Network::Parallel(c) => Network::Parallel(c.into_iter().map(Network::flip).collect()),
        }
    }

    /// flips the network so that `top()` is `node`
    fn starting_at(self, node: SignalId) -> Network {
        if self.top() == node {
            self
        } else {
            self.flip()
        }
    }

    fn join(self, other: Network, parallel: bool) -> Network {
        let mut children = Vec::new();
        for n in [self, other] {
            match (n, parallel) {
                (Network::Series(c), false) | (Network::Parallel(c), true) => children.extend(c),
                (n, _) => children.push(n),
            }
        }
        if parallel {
            Network::Parallel(children)
        } else {
            Network::Series(children)
        }
    }

    pub fn devices(&self, out: &mut Vec<usize>) {
        match self {
            Network::Device { id, .. } => out.push(*id),
            Network::Series(c) | Network::Parallel(c) => c.iter().for_each(|n| n.devices(out)),
        }
    }
}

/// Reduces the transistors of one switch network to a series-parallel tree
/// between `top` and `bottom`, by repeatedly merging
///
///   - two edges spanning the same pair of nodes  -> Parallel
///   - two edges meeting at a private inner node  -> Series
///
/// A network that is not series-parallel (a bridge) never occurs in the
/// gates built so far; it falls back to drawing the leftover edges in parallel.
pub fn extract_network(
    graph: &SignalGraph,
    transistors: &[usize],
    top: SignalId,
    bottom: SignalId,
) -> Option<Network> {
    let mut edges: Vec<Network> = transistors
        .iter()
        .map(|&t| Network::Device {
            id: t,
            gate: graph.gates[t],
            top: graph.sources[t],
            bottom: graph.drains[t],
        })
        .collect();

    let same_span = |a: &Network, b: &Network| {
        (a.top() == b.top() && a.bottom() == b.bottom())
            || (a.top() == b.bottom() && a.bottom() == b.top())
    };

    'reduce: while edges.len() > 1 {
        for i in 0..edges.len() {
            for j in (i + 1)..edges.len() {
                if same_span(&edges[i], &edges[j]) {
                    let b = edges.remove(j);
                    let a = edges.remove(i);
                    let b = b.starting_at(a.top());
                    edges.push(a.join(b, true));
                    continue 'reduce;
                }
            }
        }

        let ends: Vec<SignalId> = edges.iter().flat_map(|e| [e.top(), e.bottom()]).collect();
        for &x in &ends {
            if x == top || x == bottom || ends.iter().filter(|&&n| n == x).count() != 2 {
                continue;
            }
            let touching: Vec<usize> = (0..edges.len())
                .filter(|&i| edges[i].top() == x || edges[i].bottom() == x)
                .collect();
            if touching.len() != 2 {
                continue; // a self-loop on x
            }
            let b = edges.remove(touching[1]);
            let a = edges.remove(touching[0]);
            let a = a.flip().starting_at(x).flip(); // a ends at x
            let b = b.starting_at(x); // b starts at x
            edges.push(a.join(b, false));
            continue 'reduce;
        }
        break;
    }

    match edges.len() {
        0 => None,
        1 => edges.pop().map(|n| n.starting_at(top)),
        _ => Some(Network::Parallel(
            edges.into_iter().map(|n| n.starting_at(top)).collect(),
        )),
    }
}

// ── Canvas ───────────────────────────────────────────────────────
//
//  Everything is drawn once, at layout time, onto a grid of marks. A wire
//  cell stores which neighbours it connects to; the glyph (┃ ━ ┳ ┻ ╋ ...)
//  falls out of that bitmask, so junctions are always correct. Each mark
//  refers to a signal or transistor, and is coloured from the live graph
//  every frame.
// ──────────────────────────────────────────────────────────────

const UP: u8 = 1;
const DOWN: u8 = 2;
const LEFT: u8 = 4;
const RIGHT: u8 = 8;

#[derive(Clone, Copy, Debug)]
enum Ink {
    Label(SignalId), // text coloured by a signal's state
    Device(usize),   // transistor type letter, lit when the switch is closed
    Silk,
    Delim,
}

#[derive(Clone, Copy, Debug)]
enum Mark {
    Wire { dirs: u8, net: SignalId },
    Text { ch: char, ink: Ink },
}

fn wire_glyph(dirs: u8) -> char {
    match dirs {
        d if d == UP | DOWN | LEFT | RIGHT => '╋',
        d if d == UP | DOWN | RIGHT => '┣',
        d if d == UP | DOWN | LEFT => '┫',
        d if d == DOWN | LEFT | RIGHT => '┳',
        d if d == UP | LEFT | RIGHT => '┻',
        d if d == DOWN | RIGHT => '┏',
        d if d == DOWN | LEFT => '┓',
        d if d == UP | RIGHT => '┗',
        d if d == UP | LEFT => '┛',
        d if d & (UP | DOWN) != 0 => '┃',
        _ => '━',
    }
}

struct Canvas {
    w: u16,
    h: u16,
    marks: Vec<Option<Mark>>,
}

impl Canvas {
    fn new(w: u16, h: u16) -> Self {
        Canvas {
            w,
            h,
            marks: vec![None; w as usize * h as usize],
        }
    }

    fn get(&self, x: u16, y: u16) -> Option<Mark> {
        self.marks[y as usize * self.w as usize + x as usize]
    }

    /// adds connections to a cell; text already there is kept
    fn wire(&mut self, x: u16, y: u16, dirs: u8, net: SignalId) {
        let cell = &mut self.marks[y as usize * self.w as usize + x as usize];
        *cell = match *cell {
            Some(Mark::Wire { dirs: old, net }) => Some(Mark::Wire {
                dirs: old | dirs,
                net,
            }),
            Some(text) => Some(text),
            None => Some(Mark::Wire { dirs, net }),
        };
    }

    fn text(&mut self, x: u16, y: u16, s: &str, ink: Ink) {
        for (i, ch) in s.chars().enumerate() {
            let cx = x + i as u16;
            if cx < self.w {
                self.marks[y as usize * self.w as usize + cx as usize] =
                    Some(Mark::Text { ch, ink });
            }
        }
    }

    /// vertical wire through rows y0..=y1, open at both ends
    fn vrun(&mut self, x: u16, y0: u16, y1: u16, net: SignalId) {
        for y in y0..=y1 {
            self.wire(x, y, UP | DOWN, net);
        }
    }

    /// horizontal bus across [x0, x1]
    fn hline(&mut self, y: u16, x0: u16, x1: u16, net: SignalId) {
        for x in x0..=x1 {
            let mut dirs = 0;
            if x > x0 {
                dirs |= LEFT;
            }
            if x < x1 {
                dirs |= RIGHT;
            }
            self.wire(x, y, dirs, net);
        }
    }

    fn blit(&mut self, other: &Canvas, ox: u16, oy: u16) {
        for y in 0..other.h {
            for x in 0..other.w {
                match other.get(x, y) {
                    Some(Mark::Wire { dirs, net }) => self.wire(ox + x, oy + y, dirs, net),
                    Some(text) => {
                        self.marks[(oy + y) as usize * self.w as usize + (ox + x) as usize] =
                            Some(text)
                    }
                    None => {}
                }
            }
        }
    }
}

/// A drawn network. Current enters at `top_taps` on row 0 and leaves at
/// `bottom_taps` on the last row; `spine` is the column it hangs from.
struct Drawn {
    canvas: Canvas,
    spine: u16,
    top_taps: Vec<u16>,
    bottom_taps: Vec<u16>,
}

/// Draws a network. `open` (only ever used at the root) skips a parallel
/// network's top and bottom buses so its branches tap the rail / output row
/// directly, which is how a NAND's pull-up is normally drawn.
fn draw_network(graph: &SignalGraph, net: &Network, label_w: u16, open: bool) -> Drawn {
    match net {
        //    ┃      row 0: channel in (top node)
        //  A║P     row 1: gate label, gate oxide, device type
        Network::Device { id, gate, top, .. } => {
            let spine = label_w;
            let mut c = Canvas::new(label_w + 3, 2);
            c.wire(spine, 0, UP | DOWN, *top);
            let name = net_name(graph, *gate);
            let name_x = spine - name.chars().count() as u16;
            c.text(name_x, 1, &name, Ink::Label(*gate));
            c.text(spine, 1, "║", Ink::Label(*gate));
            let letter = match graph.kinds[*id] {
                TransistorKind::PMOS => "P",
                TransistorKind::NMOS => "N",
            };
            c.text(spine + 1, 1, letter, Ink::Device(*id));
            Drawn {
                canvas: c,
                spine,
                top_taps: vec![spine],
                bottom_taps: vec![spine],
            }
        }

        Network::Series(children) => {
            let parts: Vec<Drawn> = children
                .iter()
                .map(|n| draw_network(graph, n, label_w, false))
                .collect();
            let spine = parts.iter().map(|d| d.spine).max().unwrap_or(0);
            let w = parts
                .iter()
                .map(|d| spine - d.spine + d.canvas.w)
                .max()
                .unwrap_or(1);
            let h = parts.iter().map(|d| d.canvas.h).sum();
            let mut c = Canvas::new(w, h);
            let mut y = 0;
            for d in &parts {
                c.blit(&d.canvas, spine - d.spine, y);
                y += d.canvas.h;
            }
            Drawn {
                canvas: c,
                spine,
                top_taps: vec![spine],
                bottom_taps: vec![spine],
            }
        }

        //  ┏━━┻━━┓    top bus (skipped when open)
        //  ┃     ┃
        // A║P   B║P   branches side by side, padded to equal height
        //  ┗━━┳━━┛    bottom bus (skipped when open)
        Network::Parallel(children) => {
            let parts: Vec<Drawn> = children
                .iter()
                .map(|n| draw_network(graph, n, label_w, false))
                .collect();
            let body_h = parts.iter().map(|d| d.canvas.h).max().unwrap_or(0);
            let (bus_top, bus_bottom) = if open { (0, 0) } else { (1, 1) };
            let w = parts.iter().map(|d| d.canvas.w).sum::<u16>() + parts.len() as u16 - 1;
            let h = bus_top + body_h + bus_bottom;
            let mut c = Canvas::new(w, h);

            let mut x = 0;
            let mut spines = Vec::new();
            for (d, n) in parts.iter().zip(children) {
                c.blit(&d.canvas, x, bus_top);
                let s = x + d.spine;
                if d.canvas.h < body_h {
                    c.vrun(s, bus_top + d.canvas.h, bus_top + body_h - 1, n.bottom());
                }
                spines.push(s);
                x += d.canvas.w + 1;
            }
            let first = spines[0];
            let last = spines[spines.len() - 1];
            let spine = (first + last) / 2;

            if open {
                return Drawn {
                    canvas: c,
                    spine,
                    top_taps: spines.clone(),
                    bottom_taps: spines,
                };
            }
            c.hline(0, first, last, net.top());
            c.hline(h - 1, first, last, net.bottom());
            for &s in &spines {
                c.wire(s, 0, DOWN, net.top());
                c.wire(s, h - 1, UP, net.bottom());
            }
            c.wire(spine, 0, UP, net.top());
            c.wire(spine, h - 1, DOWN, net.bottom());
            Drawn {
                canvas: c,
                spine,
                top_taps: vec![spine],
                bottom_taps: vec![spine],
            }
        }
    }
}

// ── Gate layout ──────────────────────────────────────────────────
//
//  Each CMOS stage becomes one column. Columns share the rails and line up
//  their output rows; a bracket row on top groups the stages that belong to
//  the same gate or sub-unit.
//
//    ┌──────── XOR ────────┐            group row
//      NAND        NAND                 stage kind
//  VDD━┳━━━━┳━━━━━━━┳━━━━┳━━            VDD rail
//      ┃    ┃   │   ┃    ┃
//     A║P  B║P  │  A║P n5║P             pull-up networks
//      ┗━┳━━┻━n5│   ┗━┳━━┻━n7           output row
//       A║N     │    A║N                pull-down networks
//        ┃      │     ┃
//       B║N     │   n5║N
//  GND━━━┻━━━━━━━━━━━━┻━━━━━            GND rail
// ──────────────────────────────────────────────────────────────

const GROUP_ROW: u16 = 0;
const KIND_ROW: u16 = 1;
const VDD_ROW: u16 = 2;
const RAIL_LABEL_W: u16 = 4; // "VDD" + one rail cell

/// One CMOS stage: a pull-up and a pull-down network sharing `output`.
pub struct StageLayout {
    pub kind: GateKind,
    pub output: SignalId,
    pub pull_up: Option<Network>,
    pub pull_down: Option<Network>,
}

pub struct GateLayout {
    pub label: String,
    pub output_signal: SignalId,
    pub stages: Vec<StageLayout>,
    canvas: Canvas,
    out_row: u16,
    gnd_row: u16,
}

impl GateLayout {
    pub fn width(&self) -> u16 {
        self.canvas.w
    }

    pub fn height(&self) -> u16 {
        self.canvas.h
    }

    /// every transistor drawn, in drawing order
    pub fn devices(&self) -> Vec<usize> {
        let mut out = Vec::new();
        for s in &self.stages {
            for n in [&s.pull_up, &s.pull_down].into_iter().flatten() {
                n.devices(&mut out);
            }
        }
        out
    }
}

/// Collects the single-stage gates under a descriptor, in construction order.
fn collect_stages(graph: &SignalGraph, d: &GateDescriptor, out: &mut Vec<StageLayout>) {
    match &d.body {
        GateBody::Stage(transistors) => {
            let (pmos, nmos): (Vec<usize>, Vec<usize>) = transistors
                .iter()
                .partition(|&&t| graph.kinds[t] == TransistorKind::PMOS);
            out.push(StageLayout {
                kind: d.kind,
                output: d.output,
                pull_up: extract_network(graph, &pmos, graph.vdd(), d.output),
                pull_down: extract_network(graph, &nmos, d.output, graph.gnd()),
            });
        }
        GateBody::Composite(sub) => sub.iter().for_each(|g| collect_stages(graph, g, out)),
    }
}

pub fn layout_gate(graph: &SignalGraph, descriptor: &GateDescriptor, label: &str) -> GateLayout {
    let groups: Vec<(String, Vec<&GateDescriptor>)> = match &descriptor.body {
        GateBody::Stage(_) => vec![(descriptor.kind.name().to_string(), vec![descriptor])],
        GateBody::Composite(sub) => sub
            .iter()
            .map(|g| (g.kind.name().to_string(), vec![g]))
            .collect(),
    };
    build_layout(graph, label, descriptor.output, &groups)
}

pub fn layout_unit(graph: &SignalGraph, unit: &UnitDescriptor) -> GateLayout {
    build_layout(graph, &unit.name, unit.outputs[0], &unit.flatten())
}

fn build_layout(
    graph: &SignalGraph,
    label: &str,
    output_signal: SignalId,
    groups: &[(String, Vec<&GateDescriptor>)],
) -> GateLayout {
    // stages, and the run of stages each group covers
    let mut stages = Vec::new();
    let mut spans: Vec<(String, usize, usize)> = Vec::new();
    for (name, gates) in groups {
        let start = stages.len();
        gates
            .iter()
            .for_each(|g| collect_stages(graph, g, &mut stages));
        // a group only gets a bracket when it is made of several stages
        if stages.len() - start > 1 {
            spans.push((name.clone(), start, stages.len()));
        }
    }

    let mut label_w = 1;
    for s in &stages {
        let mut ids = Vec::new();
        for n in [&s.pull_up, &s.pull_down].into_iter().flatten() {
            n.devices(&mut ids);
        }
        for t in ids {
            label_w = label_w.max(net_name(graph, graph.gates[t]).chars().count() as u16);
        }
    }

    let drawn: Vec<(Option<Drawn>, Option<Drawn>)> = stages
        .iter()
        .map(|s| {
            let draw = |n: &Option<Network>| {
                n.as_ref().map(|n| {
                    let open = matches!(n, Network::Parallel(_));
                    draw_network(graph, n, label_w, open)
                })
            };
            (draw(&s.pull_up), draw(&s.pull_down))
        })
        .collect();

    let up_h = drawn
        .iter()
        .filter_map(|(u, _)| u.as_ref().map(|d| d.canvas.h))
        .max()
        .unwrap_or(0);
    let down_h = drawn
        .iter()
        .filter_map(|(_, d)| d.as_ref().map(|d| d.canvas.h))
        .max()
        .unwrap_or(0);

    // a little air between the rails and the networks
    let out_row = VDD_ROW + 1 + up_h + 1;
    let gnd_row = out_row + 1 + down_h + 1;
    let height = gnd_row + 1;

    // ── place each stage in its own column ─────────────────────
    struct Placed {
        x: u16,
        w: u16,
    }
    let mut placed = Vec::new();
    let mut columns = Vec::new();
    for (s, (up, down)) in stages.iter().zip(&drawn) {
        let spine = 1 + [up, down]
            .iter()
            .filter_map(|d| d.as_ref().map(|d| d.spine))
            .max()
            .unwrap_or(0);
        let name = net_name(graph, s.output);

        let mut taps_out = vec![spine];
        let mut w = s.kind.name().len() as u16 + 2;
        for (d, top) in [(up, false), (down, true)] {
            if let Some(d) = d {
                let off = spine - d.spine;
                w = w.max(off + d.canvas.w);
                let taps = if top { &d.top_taps } else { &d.bottom_taps };
                taps_out.extend(taps.iter().map(|t| off + t));
            }
        }
        let bus_end = *taps_out.iter().max().unwrap() + 1;
        w = w.max(bus_end + 1 + name.chars().count() as u16 + 1);

        let mut c = Canvas::new(w, height);
        if let Some(d) = up {
            let off = spine - d.spine;
            let y = out_row - d.canvas.h;
            c.blit(&d.canvas, off, y);
            for t in &d.top_taps {
                c.wire(off + t, VDD_ROW, DOWN, graph.vdd());
                c.vrun(off + t, VDD_ROW + 1, y - 1, graph.vdd());
            }
            for t in &d.bottom_taps {
                c.wire(off + t, out_row, UP, s.output);
            }
        }
        if let Some(d) = down {
            let off = spine - d.spine;
            let y = out_row + 1;
            c.blit(&d.canvas, off, y);
            for t in &d.top_taps {
                c.wire(off + t, out_row, DOWN, s.output);
            }
            for t in &d.bottom_taps {
                c.vrun(off + t, y + d.canvas.h, gnd_row - 1, graph.gnd());
                c.wire(off + t, gnd_row, UP, graph.gnd());
            }
        }
        let bus_start = *taps_out.iter().min().unwrap();
        c.hline(out_row, bus_start, bus_end, s.output);
        c.text(bus_end + 1, out_row, &name, Ink::Label(s.output));

        let kind = s.kind.name();
        let kx = (w.saturating_sub(kind.len() as u16)) / 2;
        c.text(kx, KIND_ROW, kind, Ink::Silk);

        columns.push(c);
        placed.push(Placed { x: 0, w });
    }

    // ── compose ────────────────────────────────────────────────
    let mut x = RAIL_LABEL_W;
    for (i, p) in placed.iter_mut().enumerate() {
        p.x = x;
        x += p.w;
        if i + 1 < stages.len() {
            x += 1; // delimiter
        }
    }
    let width = x + 1;
    let mut canvas = Canvas::new(width, height);

    canvas.text(0, VDD_ROW, "VDD", Ink::Silk);
    canvas.text(0, gnd_row, "GND", Ink::Silk);
    canvas.hline(VDD_ROW, RAIL_LABEL_W - 1, width - 1, graph.vdd());
    canvas.hline(gnd_row, RAIL_LABEL_W - 1, width - 1, graph.gnd());

    for (i, (p, c)) in placed.iter().zip(&columns).enumerate() {
        canvas.blit(c, p.x, 0);
        if i + 1 < placed.len() {
            for y in VDD_ROW + 1..gnd_row {
                canvas.text(p.x + p.w, y, "│", Ink::Delim);
            }
        }
    }

    for (name, start, end) in &spans {
        let x0 = placed[*start].x;
        let x1 = placed[end - 1].x + placed[end - 1].w - 1;
        let span = x1 - x0 + 1;
        let title = format!(" {} ", name.to_uppercase());
        let mut row: Vec<char> = vec!['─'; span as usize];
        row[0] = '┌';
        row[span as usize - 1] = '┐';
        let tx = (span.saturating_sub(title.chars().count() as u16)) / 2;
        for (i, ch) in title.chars().enumerate() {
            let at = tx as usize + i;
            if at > 0 && at + 1 < row.len() {
                row[at] = ch;
            }
        }
        let row: String = row.into_iter().collect();
        canvas.text(x0, GROUP_ROW, &row, Ink::Silk);
    }

    GateLayout {
        label: label.to_string(),
        output_signal,
        stages,
        canvas,
        out_row,
        gnd_row,
    }
}

// ── Widget ───────────────────────────────────────────────────────

pub struct TopologyWidget<'a> {
    pub graph: &'a SignalGraph,
    pub layout: &'a GateLayout,
    /// horizontal pan, in columns; clamped to the layout width
    pub scroll_x: u16,
}

impl TopologyWidget<'_> {
    fn style(&self, ink: Ink) -> Style {
        match ink {
            Ink::Label(s) => match self.graph.signals[s.0] {
                SignalState::High => Style::default().fg(GOLD_BRIGHT).bold(),
                state => wire_style(state),
            },
            Ink::Device(t) => {
                if self.graph.conducting(t) {
                    Style::default().fg(ACCENT_GOLD).bold()
                } else {
                    Style::default().fg(DEVICE_OFF)
                }
            }
            Ink::Silk => Style::default().fg(SILK_ETCH),
            Ink::Delim => Style::default().fg(GOLD_DIM),
        }
    }
}

impl Widget for TopologyWidget<'_> {
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
                Span::raw(" "),
            ]));

        let inner = block.inner(area);
        block.render(area, buf);
        if inner.width == 0 || inner.height == 0 {
            return;
        }

        let lw = self.layout.width();
        let lh = self.layout.height();

        // centre what fits, pan what doesn't
        let (dst_x, src_x, view_w) = if lw <= inner.width {
            (inner.x + (inner.width - lw) / 2, 0, lw)
        } else {
            let max_scroll = lw - inner.width;
            (inner.x, self.scroll_x.min(max_scroll), inner.width)
        };
        let (dst_y, view_h) = if lh <= inner.height {
            (inner.y + (inner.height - lh) / 2, lh)
        } else {
            (inner.y, inner.height)
        };

        let canvas = &self.layout.canvas;
        for vy in 0..view_h {
            for vx in 0..view_w {
                let (cx, cy) = (src_x + vx, vy);
                let (x, y) = (dst_x + vx, dst_y + vy);
                let (symbol, style) = match canvas.get(cx, cy) {
                    Some(Mark::Wire { dirs, net }) => (
                        wire_glyph(dirs).to_string(),
                        wire_style(self.graph.signals[net.0]),
                    ),
                    Some(Mark::Text { ch, ink }) => (ch.to_string(), self.style(ink)),
                    None if cy > VDD_ROW && cy < self.layout.gnd_row && cx >= RAIL_LABEL_W => {
                        let (tex, color) = if cy == self.layout.out_row {
                            (TRACE_TEX, SILICON_TRACE)
                        } else {
                            (ACTIVE_TEX, SILICON_ACTIVE)
                        };
                        let idx = (cx as usize + cy as usize * 7) % tex.len();
                        (tex[idx].to_string(), Style::default().fg(color))
                    }
                    None => continue,
                };
                buf[(x, y)].set_symbol(&symbol).set_style(style);
            }
        }

        // pan hints when part of the chip is off screen
        let hint = Style::default().fg(GOLD_BRIGHT).bold();
        let mid = inner.y + inner.height / 2;
        if src_x > 0 {
            buf[(inner.x, mid)].set_symbol("◀").set_style(hint);
        }
        if src_x + view_w < lw {
            buf[(inner.right() - 1, mid)]
                .set_symbol("▶")
                .set_style(hint);
        }
        if view_h < lh {
            let x = inner.x + inner.width / 2;
            buf[(x, inner.bottom() - 1)].set_symbol("▼").set_style(hint);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gallery::scenes;

    fn render(scene: &crate::gallery::Scene, w: u16, h: u16, scroll_x: u16) -> Buffer {
        let area = Rect::new(0, 0, w, h);
        let mut buf = Buffer::empty(area);
        TopologyWidget {
            graph: &scene.graph,
            layout: &scene.layout,
            scroll_x,
        }
        .render(area, &mut buf);
        buf
    }

    fn text(buf: &Buffer) -> String {
        let area = buf.area;
        (0..area.height)
            .map(|y| {
                let line: String = (0..area.width).map(|x| buf[(x, y)].symbol()).collect();
                line.trim_end().to_string()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// checks a network is wired exactly like the transistors it came from
    fn check_network(graph: &SignalGraph, net: &Network, top: SignalId, bottom: SignalId) {
        match net {
            Network::Device {
                id,
                gate,
                top: t,
                bottom: b,
            } => {
                assert_eq!((*t, *b), (top, bottom), "device {id} misplaced");
                let ends = [graph.sources[*id], graph.drains[*id]];
                assert!(ends.contains(t) && ends.contains(b), "device {id} rewired");
                assert_eq!(*gate, graph.gates[*id]);
            }
            Network::Series(c) => {
                assert!(c.len() > 1);
                let mut at = top;
                for (i, n) in c.iter().enumerate() {
                    let next = if i + 1 == c.len() { bottom } else { n.bottom() };
                    check_network(graph, n, at, next);
                    at = next;
                }
            }
            Network::Parallel(c) => {
                assert!(c.len() > 1);
                c.iter().for_each(|n| check_network(graph, n, top, bottom));
            }
        }
    }

    #[test]
    fn every_transistor_is_drawn_exactly_once() {
        for scene in scenes() {
            let mut drawn = scene.layout.devices();
            drawn.sort();
            let all: Vec<usize> = (0..scene.graph.kinds.len()).collect();
            assert_eq!(drawn, all, "{}", scene.layout.label);
        }
    }

    #[test]
    fn networks_match_the_netlist() {
        for scene in scenes() {
            let g = &scene.graph;
            for stage in &scene.layout.stages {
                let up = stage.pull_up.as_ref().expect("stage without pull-up");
                let down = stage.pull_down.as_ref().expect("stage without pull-down");
                check_network(g, up, g.vdd(), stage.output);
                check_network(g, down, stage.output, g.gnd());
            }
        }
    }

    #[test]
    fn nand_and_nor_are_duals() {
        let scenes = scenes();
        let find = |label: &str| {
            scenes
                .iter()
                .find(|s| s.layout.label == label)
                .unwrap()
                .layout
                .stages
                .first()
                .unwrap()
        };
        let nand = find("NAND");
        assert!(matches!(nand.pull_up, Some(Network::Parallel(_))));
        assert!(matches!(nand.pull_down, Some(Network::Series(_))));
        let nor = find("NOR");
        assert!(matches!(nor.pull_up, Some(Network::Series(_))));
        assert!(matches!(nor.pull_down, Some(Network::Parallel(_))));
    }

    #[test]
    fn full_view_shows_one_gate_per_transistor() {
        for scene in scenes() {
            let (w, h) = (scene.layout.width() + 2, scene.layout.height() + 2);
            let buf = render(&scene, w, h, 0);
            let gates = buf.content().iter().filter(|c| c.symbol() == "║").count();
            assert_eq!(gates, scene.graph.kinds.len(), "{}", scene.layout.label);
        }
    }

    #[test]
    fn renders_at_any_size_without_panicking() {
        for scene in scenes() {
            for (w, h) in [(1, 1), (3, 3), (12, 6), (40, 12), (80, 24), (300, 60)] {
                for scroll in [0, 7, 40, u16::MAX] {
                    render(&scene, w, h, scroll);
                }
            }
        }
    }

    /// prints every circuit; run with `cargo test dump -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn dump() {
        for scene in scenes() {
            let (w, h) = (scene.layout.width() + 2, scene.layout.height() + 2);
            println!("{}\n", text(&render(&scene, w, h, 0)));
        }
    }
}
