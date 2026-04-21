# codename: vault -> the virtual computer

This is not a virtual 'machine'. It is a virtual computer. Starting from
software transistors, we simulate the computer from first principles

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
