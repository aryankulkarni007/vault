use std::collections::HashMap;
use std::collections::VecDeque;
use std::vec;

/// index for signals
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct SignalId(pub usize);

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum SignalState {
    High,
    Low,
    Floating, // undriven wire (power on, but not yet initalised state)
    Conflict, // two drivers fighting - short circuit (visual needed)
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct TransistorId(usize);

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum TransistorKind {
    NMOS,
    PMOS,
}

pub struct SignalGraph {
    // signal data
    pub signals: Vec<SignalState>,
    pub driven: Vec<Option<SignalState>>,
    pub signal_names: Vec<Option<String>>,
    // transistor data
    pub kinds: Vec<TransistorKind>,
    pub gates: Vec<SignalId>,
    pub sources: Vec<SignalId>,
    pub drains: Vec<SignalId>,
    pub eval_order: Vec<TransistorId>, // for kahn's algorithm,
    pub dirty: bool,                   // for lazy re-eval
}

/// for visualisation
pub struct GateDescriptor {
    pub output: SignalId,
    pub transistors: Vec<usize>, // transistor indices
}

impl SignalGraph {
    /// sets up signal graph initial state
    /// push vdd and gnd into 'signals'
    pub fn new() -> Self {
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
    pub fn add_signal(&mut self, name: Option<&str>) -> SignalId {
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

    pub fn drive(&mut self, id: SignalId, state: Option<SignalState>) {
        self.driven[id.0] = state;
    }

    /// (signal graph -> dependents, in_degree -> eval_order)
    fn kahn(&mut self) {
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
        let (dependents, mut in_degree) = self.drains.iter().enumerate().fold(
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

        // merged compute eval_order from here
        self.eval_order.clear();
        let mut eval_queue: VecDeque<usize> = in_degree
            .iter()
            .enumerate()
            .filter(|x| *x.1 == 0)
            .map(|(i, _)| i)
            .collect();
        while let Some(i) = eval_queue.pop_front() {
            self.eval_order.push(TransistorId(i));
            dependents[i].iter().for_each(|j| {
                in_degree[*j] -= 1;
                if in_degree[*j] == 0 {
                    eval_queue.push_back(*j);
                }
            });
        }
        if self.eval_order.len() < self.kinds.len() {
            println!("cycles exist");
        }
        self.dirty = false;
    }

    fn resolve(a: SignalState, b: SignalState) -> SignalState {
        match (a, b) {
            (SignalState::High, SignalState::High) => SignalState::High,
            (SignalState::High, SignalState::Low) => SignalState::Conflict,
            (SignalState::High, SignalState::Floating) => SignalState::High,
            (SignalState::Low, SignalState::High) => SignalState::Conflict,
            (SignalState::Low, SignalState::Low) => SignalState::Low,
            (SignalState::Low, SignalState::Floating) => SignalState::Low,
            (SignalState::Floating, SignalState::High) => SignalState::High,
            (SignalState::Floating, SignalState::Low) => SignalState::Low,
            (SignalState::Floating, SignalState::Floating) => SignalState::Floating,
            (_, SignalState::Conflict) => SignalState::Conflict,
            (SignalState::Conflict, _) => SignalState::Conflict,
        }
    }

    pub fn propagate(&mut self) {
        // step 1: recompute eval_order if dirty
        if self.dirty {
            self.kahn();
        }

        // step 2: reset all non-externally-driven signals to Floating
        for i in 0..self.signals.len() {
            if let Some(state) = self.driven[i] {
                self.signals[i] = state;
            } else {
                self.signals[i] = SignalState::Floating;
            }
        }

        // safety
        let mut iterations = 0;
        let max_iterations = self.kinds.len() + 1;

        // step 3: evaluate transistors in eval_order
        // for each transistor;
        // -    check gate state
        // -    determine conductivity based on kind
        // -    if conducting, resolve source and drain states
        loop {
            // NOTE: loop is for iterative relaxation
            // deals with the issue of cyclic dependency
            let prev = self.signals.clone();
            for i in 0..self.eval_order.len() {
                let transistor_idx = self.eval_order[i].0;
                let kind = self.kinds[transistor_idx];
                let gate = self.gates[transistor_idx].0;
                let source = self.sources[transistor_idx].0;
                let drain = self.drains[transistor_idx].0;

                let gate_state = self.signals[gate];

                let conducting = match kind {
                    TransistorKind::NMOS => gate_state == SignalState::High,
                    TransistorKind::PMOS => gate_state == SignalState::Low,
                };

                if conducting {
                    let resolved = SignalGraph::resolve(self.signals[source], self.signals[drain]);
                    self.signals[source] = resolved;
                    self.signals[drain] = resolved;
                }
            }
            if self.signals == prev || iterations >= max_iterations {
                break;
            }
            iterations += 1;
        }

        // step 4: apply external drives
        for i in 0..self.signals.len() {
            if let Some(state) = self.driven[i] {
                self.signals[i] = state;
            }
        }
    }

    fn vdd(&self) -> SignalId {
        SignalId(0)
    }
    fn gnd(&self) -> SignalId {
        SignalId(1)
    }
}

// logic gates
impl SignalGraph {
    pub fn nand(&mut self, a: SignalId, b: SignalId) -> GateDescriptor {
        // VDD ──┬──────────┐            nand architecture
        // [PMOS_1,A] [PMOS_2,B]
        //    └──────────┘
        //         |
        //        OUT
        //         |
        //    [NMOS_1,A]
        //         |
        //    [NMOS_2,B]
        //         |
        //        GND
        let out = self.add_signal(Some("OUT"));
        let mid = self.add_signal(None);
        let first = self.kinds.len();
        self.add_transistor(TransistorKind::PMOS, a, self.vdd(), out);
        self.add_transistor(TransistorKind::PMOS, b, self.vdd(), out);
        self.add_transistor(TransistorKind::NMOS, a, mid, out);
        self.add_transistor(TransistorKind::NMOS, b, self.gnd(), mid);
        let last = self.kinds.len();

        GateDescriptor {
            output: out,
            transistors: (first..last).collect(),
        }
    }

    pub fn not(&mut self, a: SignalId) -> GateDescriptor {
        // VDD           not architecture
        //  |
        // [PMOS, A]
        //  |
        // OUT
        //  |
        // [NMOS, A]
        //  |
        // GND
        let out = self.add_signal(Some("OUT"));
        let first = self.kinds.len();
        self.add_transistor(TransistorKind::PMOS, a, self.vdd(), out);
        self.add_transistor(TransistorKind::NMOS, a, self.gnd(), out);
        let last = self.kinds.len();

        GateDescriptor {
            output: out,
            transistors: (first..last).collect(),
        }
    }

    pub fn and(&mut self, a: SignalId, b: SignalId) -> GateDescriptor {
        // AND (NAND + NOT = 6 transistors)
        let nand = self.nand(a, b);
        let not = self.not(nand.output);

        GateDescriptor {
            output: not.output,
            transistors: nand
                .transistors
                .into_iter()
                .chain(not.transistors)
                .collect(),
        }
    }

    pub fn nor(&mut self, a: SignalId, b: SignalId) -> GateDescriptor {
        // NOR (4 transistors):
        // VDD ──┬──────────┐
        // [PMOS,A]    [PMOS,B]  ← series
        //    |           |
        // [PMOS,B]
        //    |
        //   OUT
        //    |
        // [NMOS,A]    [NMOS,B]  ← parallel
        //    |           |
        //   GND ─────────┘
        let out = self.add_signal(Some("OUT"));
        let mid = self.add_signal(None);
        let first = self.kinds.len();
        self.add_transistor(TransistorKind::PMOS, a, self.vdd(), mid);
        self.add_transistor(TransistorKind::PMOS, b, mid, out);
        self.add_transistor(TransistorKind::NMOS, a, self.gnd(), out);
        self.add_transistor(TransistorKind::NMOS, b, self.gnd(), out);
        let last = self.kinds.len();

        GateDescriptor {
            output: out,
            transistors: (first..last).collect(),
        }
    }

    pub fn or(&mut self, a: SignalId, b: SignalId, small: bool) -> GateDescriptor {
        // small=true: NOR + NOT (6 transistors)
        // small=false: De Morgan's (8 transistors)
        if small {
            let nor = self.nor(a, b);
            let not = self.not(nor.output);
            GateDescriptor {
                output: not.output,
                transistors: nor.transistors.into_iter().chain(not.transistors).collect(),
            }
        } else {
            let not_a = self.not(a);
            let not_b = self.not(b);
            let nand = self.nand(not_a.output, not_b.output);
            GateDescriptor {
                output: nand.output,
                transistors: not_a
                    .transistors
                    .into_iter()
                    .chain(not_b.transistors)
                    .chain(nand.transistors)
                    .collect(),
            }
        }
    }

    pub fn xor(&mut self, a: SignalId, b: SignalId) -> GateDescriptor {
        let s1 = self.nand(a, b);
        let s2 = self.nand(a, s1.output);
        let s3 = self.nand(b, s1.output);
        let xor = self.nand(s2.output, s3.output);
        GateDescriptor {
            output: xor.output,
            transistors: s1
                .transistors
                .into_iter()
                .chain(s2.transistors)
                .chain(s3.transistors)
                .chain(xor.transistors)
                .collect(),
        }
    }

    pub fn xnor(&mut self, a: SignalId, b: SignalId) -> GateDescriptor {
        let xor = self.xor(a, b);
        let xnor = self.not(xor.output);
        GateDescriptor {
            output: xnor.output,
            transistors: xor
                .transistors
                .into_iter()
                .chain(xnor.transistors)
                .collect(),
        }
    }
}
