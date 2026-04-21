use std::collections::HashMap;
use std::collections::VecDeque;
use std::vec;

/// index for signals
#[derive(Clone, Copy, PartialEq)]
struct SignalId(usize);

#[derive(Clone, Copy, PartialEq)]
enum SignalState {
    High,
    Low,
    Floating, // undriven wire (power on, but not yet initalised state)
    Conflict, // two drivers fighting - short circuit (visual needed)
}

#[derive(Clone, Copy, PartialEq)]
struct TransistorId(usize);

#[derive(Clone, Copy, PartialEq)]
enum TransistorKind {
    NMOS,
    PMOS,
}

struct SignalGraph {
    // signal data
    signals: Vec<SignalState>,
    driven: Vec<Option<SignalState>>,
    signal_names: Vec<Option<String>>,
    // transistor data
    kinds: Vec<TransistorKind>,
    gates: Vec<SignalId>,
    sources: Vec<SignalId>,
    drains: Vec<SignalId>,
    eval_order: Vec<TransistorId>, // for kahn's algorithm,
    dirty: bool,                   // for lazy re-eval
}

impl SignalGraph {
    /// sets up signal graph initial state
    /// push vdd and gnd into 'signals'
    fn new() -> Self {
        let vdd = SignalState::High;
        let gnd = SignalState::Low;
        let signals = vec![vdd, gnd];
        let driven = vec![Some(vdd), Some(gnd)];
        let signal_names = vec![Some("VDD".to_string()), Some("GND".to_string())];
        SignalGraph {
            signals,
            driven,
            signal_names,
            kinds: Vec::new(),
            gates: Vec::new(),
            sources: Vec::new(),
            drains: Vec::new(),
            eval_order: Vec::new(),
            dirty: false,
        }
    }

    /// adds floating signal -> signal id
    fn add_signal(&mut self, name: Option<&str>) -> SignalId {
        let id = self.signals.len();
        self.signals.push(SignalState::Floating);
        self.driven.push(None);
        self.signal_names.push(name.map(|s| s.to_string()));
        SignalId(id)
    }

    /// connects the gate, source and the drain and pushes transistor
    fn add_transistor(
        &mut self,
        kind: TransistorKind,
        gate: SignalId,
        source: SignalId,
        drain: SignalId,
    ) -> TransistorId {
        let id = self.kinds.len();
        self.kinds.push(kind);
        self.gates.push(gate);
        self.sources.push(source);
        self.drains.push(drain);
        self.dirty = true;
        TransistorId(id)
    }

    fn drive(&mut self, id: SignalId, state: Option<SignalState>) {
        self.driven[id.0] = state;
    }

    /// (signal graph -> dependents, in_degree)
    /// recompute self.eval_order internally
    fn build_dep_map(&self) -> (Vec<Vec<usize>>, Vec<usize>) {
        // step 1: build a hashmap that stores the gates at a specific signal
        let signal_to_transistors = self.gates.iter().enumerate().fold(
            HashMap::new(),
            |mut map: HashMap<usize, Vec<usize>>,
             (transistor_idx, signal_id): (usize, &SignalId)| {
                // for each pair, push transistor_idx to map[signal_id]
                map.entry(signal_id.0).or_default().push(transistor_idx);
                map
            },
        );

        // step 2
        let n = self.kinds.len();
        let (dependents, in_degree) = self.drains.iter().enumerate().fold(
            (vec![vec![]; n], vec![0usize; n]),
            |mut acc, (i, drain_signal)| {
                // reindex the signal_to_transistors vec to be
                // indexed by transistor instead of signl
                acc.0[i] = signal_to_transistors
                    .get(&drain_signal.0)
                    .cloned()
                    .unwrap_or_default();
                acc.0[i].iter().for_each(|j| acc.1[*j] += 1);
                acc
            },
        );
        (dependents, in_degree)
    }
}

fn main() {
    println!("Hello, world!");
}
