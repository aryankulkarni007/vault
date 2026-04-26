use crate::sim::gate::GateKind;
use std::collections::HashMap;
use std::collections::HashSet;
use std::collections::VecDeque;

/// index for signals
#[derive(Clone, Copy, PartialEq, Debug, Eq, Hash)]
pub struct SignalId(pub usize);

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum SignalState {
    High,
    Low,
    Floating,
    Conflict,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct TransistorId(pub usize);

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum TransistorKind {
    NMOS,
    PMOS,
}

#[derive(Clone)]
pub struct CapacitorState {
    pub charge: u8,
    pub threshold: u8,
    pub period: u8,
    pub charging: bool,
}

pub struct SignalGraph {
    // signal data
    pub signals: Vec<SignalState>,
    pub driven: Vec<Option<SignalState>>,
    pub signal_names: Vec<Option<String>>,
    pub sequential: Vec<SignalId>, // signals that persist state across propagations
    // transistor data
    pub kinds: Vec<TransistorKind>,
    pub gates: Vec<SignalId>,
    pub sources: Vec<SignalId>,
    pub drains: Vec<SignalId>,
    pub eval_order: Vec<TransistorId>,
    pub dirty: bool,
    pub capacitors: Vec<Option<CapacitorState>>,
    pub cycle_count: u64,
}

pub struct GateDescriptor {
    pub kind: GateKind,
    pub inputs: Vec<SignalId>,
    pub output: SignalId,
    pub transistors: Option<Vec<usize>>,
    pub sub_gates: Option<Vec<GateDescriptor>>,
}

impl SignalGraph {
    pub fn new() -> Self {
        let vdd = SignalState::High;
        let gnd = SignalState::Low;
        SignalGraph {
            signals: vec![vdd, gnd],
            driven: vec![Some(vdd), Some(gnd)],
            signal_names: vec![Some("VDD".to_string()), Some("GND".to_string())],
            sequential: Vec::new(),
            kinds: Vec::new(),
            gates: Vec::new(),
            sources: Vec::new(),
            drains: Vec::new(),
            eval_order: Vec::new(),
            dirty: false,
            capacitors: vec![None, None],
            cycle_count: 0,
        }
    }

    pub fn add_signal(&mut self, name: Option<&str>) -> SignalId {
        let id = SignalId(self.signals.len());
        self.signals.push(SignalState::Floating);
        self.driven.push(None);
        self.signal_names.push(name.map(|s| s.to_string()));
        self.capacitors.push(None);
        id
    }

    /// signal that persists state across propagations (Q, !Q, register outputs)
    pub fn add_sequential_signal(&mut self, name: Option<&str>) -> SignalId {
        let id = self.add_signal(name);
        self.sequential.push(id);
        id
    }

    pub fn add_transistor(
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

    pub fn add_clock(&mut self, period: u8) -> SignalId {
        let id = self.add_signal(Some("CLK"));
        let threshold = period / 2;
        self.capacitors[id.0] = Some(CapacitorState {
            charge: 0,
            threshold,
            period,
            charging: true,
        });
        self.signals[id.0] = SignalState::Low;
        self.driven[id.0] = Some(SignalState::Low);
        id
    }

    pub fn drive(&mut self, id: SignalId, state: Option<SignalState>) {
        self.driven[id.0] = state;
    }

    pub fn vdd(&self) -> SignalId {
        SignalId(0)
    }
    pub fn gnd(&self) -> SignalId {
        SignalId(1)
    }

    fn kahn(&mut self) {
        let signal_to_transistors = self.gates.iter().enumerate().fold(
            HashMap::new(),
            |mut map: HashMap<usize, Vec<usize>>, (t_idx, sig_id)| {
                map.entry(sig_id.0).or_default().push(t_idx);
                map
            },
        );

        let n = self.kinds.len();
        let (dependents, mut in_degree) = self.drains.iter().enumerate().fold(
            (vec![vec![]; n], vec![0usize; n]),
            |mut acc, (i, drain_signal)| {
                acc.0[i] = signal_to_transistors
                    .get(&drain_signal.0)
                    .cloned()
                    .unwrap_or_default();
                acc.0[i].iter().for_each(|j| acc.1[*j] += 1);
                acc
            },
        );

        self.eval_order.clear();
        let mut queue: VecDeque<usize> = in_degree
            .iter()
            .enumerate()
            .filter(|&(_idx, &degree)| degree == 0)
            .map(|(i, _)| i)
            .collect();

        while let Some(i) = queue.pop_front() {
            self.eval_order.push(TransistorId(i));
            for &j in &dependents[i] {
                in_degree[j] -= 1;
                if in_degree[j] == 0 {
                    queue.push_back(j);
                }
            }
        }

        if self.eval_order.len() < self.kinds.len() {
            // cycles exist — sequential circuit
        }
        self.dirty = false;
    }

    fn resolve(a: SignalState, b: SignalState) -> SignalState {
        match (a, b) {
            (SignalState::High, SignalState::High) => SignalState::High,
            (SignalState::Low, SignalState::Low) => SignalState::Low,
            (SignalState::High, SignalState::Floating) => SignalState::High,
            (SignalState::Low, SignalState::Floating) => SignalState::Low,
            (SignalState::Floating, SignalState::High) => SignalState::High,
            (SignalState::Floating, SignalState::Low) => SignalState::Low,
            (SignalState::Floating, SignalState::Floating) => SignalState::Floating,
            (SignalState::High, SignalState::Low) => SignalState::Conflict,
            (SignalState::Low, SignalState::High) => SignalState::Conflict,
            (_, SignalState::Conflict) => SignalState::Conflict,
            (SignalState::Conflict, _) => SignalState::Conflict,
        }
    }

    pub fn propagate(&mut self) {
        if self.dirty {
            self.kahn();
        }

        let eval_set: HashSet<usize> = self.eval_order.iter().map(|t| t.0).collect();
        let has_cycles = eval_set.len() < self.kinds.len();

        // build sequential set for O(1) lookup
        let sequential_set: HashSet<usize> = self.sequential.iter().map(|s| s.0).collect();

        // reset phase: sequential signals preserve state, everything else floats
        for i in 0..self.signals.len() {
            if let Some(state) = self.driven[i] {
                self.signals[i] = state;
            } else if !sequential_set.contains(&i) {
                self.signals[i] = SignalState::Floating;
            }
            // sequential non-driven signals keep current value
        }

        // build full eval order: topo-sorted first, cyclic remainder after — O(n)
        let mut in_order = Vec::with_capacity(self.kinds.len());
        let mut remainder = Vec::new();
        for i in 0..self.kinds.len() {
            if eval_set.contains(&i) {
                in_order.push(i);
            } else {
                remainder.push(i);
            }
        }
        // eval_order is already topo-sorted, use it directly
        let full_order: Vec<usize> = self
            .eval_order
            .iter()
            .map(|&TransistorId(i)| i)
            .chain(remainder)
            .collect();

        let max_iterations = if has_cycles {
            self.kinds.len() * 4
        } else {
            self.kinds.len() + 1
        };

        let mut iterations = 0;
        loop {
            let prev = self.signals.clone();
            for &t_idx in &full_order {
                let kind = self.kinds[t_idx];
                let gate = self.gates[t_idx].0;
                let source = self.sources[t_idx].0;
                let drain = self.drains[t_idx].0;

                let conducting = match kind {
                    TransistorKind::NMOS => self.signals[gate] == SignalState::High,
                    TransistorKind::PMOS => self.signals[gate] == SignalState::Low,
                };

                if conducting {
                    let src_driven = self.driven[source].is_some();
                    let drn_driven = self.driven[drain].is_some();
                    if src_driven && !drn_driven {
                        self.signals[drain] = self.signals[source];
                    } else if drn_driven && !src_driven {
                        self.signals[source] = self.signals[drain];
                    } else {
                        let resolved = Self::resolve(self.signals[source], self.signals[drain]);
                        if !src_driven {
                            self.signals[source] = resolved;
                        }
                        if !drn_driven {
                            self.signals[drain] = resolved;
                        }
                    }
                }
            }
            iterations += 1;
            if self.signals == prev || iterations >= max_iterations {
                break;
            }
        }

        // re-apply drives — always win
        for i in 0..self.signals.len() {
            if let Some(state) = self.driven[i] {
                self.signals[i] = state;
            }
        }
    }

    pub fn tick(&mut self) {
        for (sid, cap_opt) in self.capacitors.iter_mut().enumerate() {
            if let Some(cap) = cap_opt {
                if cap.charging {
                    cap.charge += 1;
                    if cap.charge >= cap.period {
                        cap.charging = false;
                    }
                } else {
                    if cap.charge > 0 {
                        cap.charge -= 1;
                    }
                    if cap.charge == 0 {
                        cap.charging = true;
                    }
                }
                let state = if cap.charge >= cap.threshold {
                    SignalState::High
                } else {
                    SignalState::Low
                };
                self.signals[sid] = state;
                self.driven[sid] = Some(state);
            }
        }
        self.cycle_count += 1;
        self.propagate();
    }
}

impl SignalGraph {
    pub fn nand(&mut self, a: SignalId, b: SignalId) -> GateDescriptor {
        let out = self.add_signal(Some("OUT"));
        let mid = self.add_signal(None);
        let first = self.kinds.len();
        self.add_transistor(TransistorKind::PMOS, a, self.vdd(), out);
        self.add_transistor(TransistorKind::PMOS, b, self.vdd(), out);
        self.add_transistor(TransistorKind::NMOS, a, mid, out);
        self.add_transistor(TransistorKind::NMOS, b, self.gnd(), mid);
        let last = self.kinds.len();
        GateDescriptor {
            kind: GateKind::NAND,
            inputs: vec![a, b],
            output: out,
            transistors: Some((first..last).collect()),
            sub_gates: None,
        }
    }

    pub fn not(&mut self, a: SignalId) -> GateDescriptor {
        let out = self.add_signal(Some("OUT"));
        let first = self.kinds.len();
        self.add_transistor(TransistorKind::PMOS, a, self.vdd(), out);
        self.add_transistor(TransistorKind::NMOS, a, self.gnd(), out);
        let last = self.kinds.len();
        GateDescriptor {
            kind: GateKind::NOT,
            inputs: vec![a],
            output: out,
            transistors: Some((first..last).collect()),
            sub_gates: None,
        }
    }

    pub fn and(&mut self, a: SignalId, b: SignalId) -> GateDescriptor {
        let nand = self.nand(a, b);
        let not = self.not(nand.output);
        GateDescriptor {
            kind: GateKind::AND,
            inputs: vec![a, b],
            output: not.output,
            transistors: None,
            sub_gates: Some(vec![nand, not]),
        }
    }

    pub fn nor(&mut self, a: SignalId, b: SignalId) -> GateDescriptor {
        let out = self.add_signal(Some("OUT"));
        let mid = self.add_signal(None);
        let first = self.kinds.len();
        self.add_transistor(TransistorKind::PMOS, a, self.vdd(), mid);
        self.add_transistor(TransistorKind::PMOS, b, mid, out);
        self.add_transistor(TransistorKind::NMOS, a, self.gnd(), out);
        self.add_transistor(TransistorKind::NMOS, b, self.gnd(), out);
        let last = self.kinds.len();
        GateDescriptor {
            kind: GateKind::NOR,
            inputs: vec![a, b],
            output: out,
            transistors: Some((first..last).collect()),
            sub_gates: None,
        }
    }

    pub fn or(&mut self, a: SignalId, b: SignalId, small: bool) -> GateDescriptor {
        if small {
            let nor = self.nor(a, b);
            let not = self.not(nor.output);
            GateDescriptor {
                kind: GateKind::OR,
                inputs: vec![a, b],
                output: not.output,
                transistors: None,
                sub_gates: Some(vec![nor, not]),
            }
        } else {
            let not_a = self.not(a);
            let not_b = self.not(b);
            let nand = self.nand(not_a.output, not_b.output);
            GateDescriptor {
                kind: GateKind::OR,
                inputs: vec![a, b],
                output: nand.output,
                transistors: None,
                sub_gates: Some(vec![not_a, not_b, nand]),
            }
        }
    }

    pub fn xor(&mut self, a: SignalId, b: SignalId) -> GateDescriptor {
        let s1 = self.nand(a, b);
        let s2 = self.nand(a, s1.output);
        let s3 = self.nand(b, s1.output);
        let xor = self.nand(s2.output, s3.output);
        GateDescriptor {
            kind: GateKind::XOR,
            inputs: vec![a, b],
            output: xor.output,
            transistors: None,
            sub_gates: Some(vec![s1, s2, s3, xor]),
        }
    }

    pub fn xnor(&mut self, a: SignalId, b: SignalId) -> GateDescriptor {
        let xor = self.xor(a, b);
        let xnor = self.not(xor.output);
        GateDescriptor {
            kind: GateKind::XNOR,
            inputs: vec![a, b],
            output: xnor.output,
            transistors: None,
            sub_gates: Some(vec![xor, xnor]),
        }
    }
}
