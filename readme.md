# codename: vault -> the virtual computer

This is not a virtual 'machine'. It is a virtual computer. Starting from
software transistors, we simulate the computer from first principles. So far,
we have the fundamentals. We have a working NAND gate and a half-working
visualisation (we can flip a and b inputs using a and b :))

```rust
/*
 * example of non-data oriented design and trait usage by implementing the
 * index trait for signal graph, indexing the transistors vector is simpler
 * -> simply self.transistors[TransistorId] Removed -> it is for an
 * Vec<Transistor> not data-oriented design
 *
 */
// for indexing transistors
struct TransistorId(u32);
/// a simplified software transistor (not analogue)
struct Transistor {
    kind: TransistorKind,
    gate: SignalId,
    source: SignalId,
    drain: SignalId,
}

struct SignalGraph {
    signals: Vec<SignalState>,
    driven: Vec<Option<SignalState>>,
    transistors: Vec<Transistor>,
    names: Vec<Option<String>>,
    eval_order: Vec<TransistorId>,
    dirty: bool, // for lazy re-eval
}

impl Index<TransistorId> for SignalGraph {
    type Output = Transistor;
    fn index(&self, index: TransistorId) -> &Self::Output {
        &self.transistors[index.0 as usize]
    }
}
```

# Kahn's algorithm

1. For every transistor, count how many other transistors it depends on.
   This is its "in-degree"

2. Put all transistors with in-degree 0 into a queue.
   These are safe to evaluate immediately.

3. Take a transistor off the queue. Add it to eval_order.
   Its drain now has a resolved state.
   Find every transistor whose gate depends on that drain signal.
   Decrement their in-degree by 1.
   If any hit in-degree 0, add them to the queue.

4. Repeat step 3 until the queue is empty.

# find_clusters

O(n^2) implementation:

```rust
pub fn find_clusters(graph: &SignalGraph, transistors: &[usize]) -> Vec<Vec<usize>> {
    // we need to a union find on transistors index
    // transistors are in the same cluster if
    // they share any signal that isn't gnd or vdd
    let n = transistors.len();
    let mut parent: Vec<usize> = (0..n).collect();

    /// this function recursively calls itself to find the
    /// 'oldest relative' to the current child by walking up the tree
    fn find(parent: &mut Vec<usize>, x: usize) -> usize {
        if parent[x] != x {
            parent[x] = find(parent, parent[x]);
        }
        parent[x]
    }
    /// unions two transistors to the parent of the first
    fn union(parent: &mut Vec<usize>, x: usize, y: usize) {
        let px = find(parent, x);
        let py = find(parent, y);
        parent[px] = py;
    }

    for i in 0..n {
        for j in (i + 1)..n {
            let ti = transistors[i];
            let tj = transistors[j];
            let sigs_i = [graph.gates[ti], graph.sources[ti], graph.drains[ti]];
            let sigs_j = [graph.gates[tj], graph.sources[tj], graph.drains[tj]];
            let shared = sigs_i.iter().any(|s| {
                // > 1 skips vdd and gnd
                s.0 > 1 && sigs_j.contains(s)
            });
            if shared {
                union(&mut parent, i, j);
            }
        }
    }
    let mut groups: HashMap<usize, Vec<usize>> = HashMap::new();
    (0..n).for_each(|i| {
        groups
            .entry(find(&mut parent, i))
            .or_default()
            .push(transistors[i]);
    });
    groups.into_values().collect()
}
```

# working render() (doesn't support clusters)

```rust
    fn render(self, area: Rect, buf: &mut Buffer) {
        // STATE & STYLING PREP
        let output_state = self.graph.signals[self.layout.output_signal.0];
        let border_color = wire_style(output_state);
        let title_style = Style::default().fg(Color::Rgb(255, 200, 0));

        // BLOCK RENDER
        // We use a Line to mix styles: border characters inherit block style, text is gold.
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(border_color)
            .title(Line::from(vec![
                Span::raw("─ "),
                Span::styled(self.layout.label.to_uppercase(), title_style),
                Span::raw(" "),
            ]));

        // calculate the inner area
        let inner = block.inner(area);
        block.render(area, buf);

        // if the window is too tiny to even show the inner area, abort to prevent panics.
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

        // content width: space for gate labels + width of all pmos columns
        let content_w = GATE_WIRE_W + (p_count.max(1) * CELL_W);
        // content height: vdd + gap + pmos + merge + (nmos stack) + gnd
        let content_h = 4 + (n_count * ROW_H) + 1;

        // CENTERING & CLIPPING LOGIC
        // we calculate offsets relative to 'inner' area.
        let offset_x = inner.x + inner.width.saturating_sub(content_w) / 2;
        let offset_y = inner.y + inner.height.saturating_sub(content_h) / 2;

        // boundary check helper: prevents writing outside the widget's allocated area.
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

        //  DRAW PMOS ARRAY (Top)
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
```
