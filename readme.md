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
