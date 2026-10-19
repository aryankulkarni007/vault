#![allow(clippy::upper_case_acronyms)]
use crate::sim::gate::GateKind;

/// index for signals
#[derive(Clone, Copy, PartialEq, Debug, Eq, Hash)]
pub struct SignalId(pub usize);

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum SignalState {
    High,
    Low,
    Floating, // no path to a driver and no stored charge
    Conflict, // a net connected to drivers that disagree (short circuit)
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
    // transistor data
    pub kinds: Vec<TransistorKind>,
    pub gates: Vec<SignalId>,
    pub sources: Vec<SignalId>,
    pub drains: Vec<SignalId>,
    pub capacitors: Vec<Option<CapacitorState>>,
    pub cycle_count: u64,
    /// false when the last propagate() hit its iteration cap without reaching a
    /// fixed point (an oscillating or metastable circuit)
    pub settled: bool,
}

/// How a gate is physically built.
pub enum GateBody {
    /// one CMOS stage: a PMOS pull-up and an NMOS pull-down sharing one output
    Stage(Vec<usize>),
    /// built out of other gates
    Composite(Vec<GateDescriptor>),
}

pub struct GateDescriptor {
    pub kind: GateKind,
    pub output: SignalId,
    pub body: GateBody,
}

impl SignalGraph {
    pub fn new() -> Self {
        let vdd = SignalState::High;
        let gnd = SignalState::Low;
        SignalGraph { signals: vec![vdd, gnd],
                      driven: vec![Some(vdd), Some(gnd)],
                      signal_names: vec![Some("VDD".to_string()), Some("GND".to_string())],
                      kinds: Vec::new(),
                      gates: Vec::new(),
                      sources: Vec::new(),
                      drains: Vec::new(),
                      capacitors: vec![None, None],
                      cycle_count: 0,
                      settled: true }
    }

    pub fn add_signal(&mut self, name: Option<&str>) -> SignalId {
        let id = SignalId(self.signals.len());
        self.signals.push(SignalState::Floating);
        self.driven.push(None);
        self.signal_names.push(name.map(|s| s.to_string()));
        self.capacitors.push(None);
        id
    }

    /// names (or renames) a signal, e.g. to label a circuit's outputs
    pub fn name(&mut self, id: SignalId, name: &str) {
        self.signal_names[id.0] = Some(name.to_string());
    }

    pub fn add_transistor(&mut self,
                          kind: TransistorKind,
                          gate: SignalId,
                          source: SignalId,
                          drain: SignalId)
                          -> TransistorId {
        let id = self.kinds.len();
        self.kinds.push(kind);
        self.gates.push(gate);
        self.sources.push(source);
        self.drains.push(drain);
        TransistorId(id)
    }

    pub fn add_clock(&mut self, period: u8) -> SignalId {
        let id = self.add_signal(Some("CLK"));
        let threshold = period / 2;
        self.capacitors[id.0] = Some(CapacitorState { charge: 0,
                                                      threshold,
                                                      period,
                                                      charging: true });
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

    /// a transistor is a switch: NMOS closes on a HIGH gate, PMOS on a LOW gate.
    /// a floating or conflicted gate leaves the switch open.
    pub fn conducting(&self, t: usize) -> bool {
        let gate = self.signals[self.gates[t].0];
        match self.kinds[t] {
            TransistorKind::NMOS => gate == SignalState::High,
            TransistorKind::PMOS => gate == SignalState::Low,
        }
    }

    /// combines two drivers on the same net
    fn resolve(a: SignalState, b: SignalState) -> SignalState {
        use SignalState::*;
        match (a, b) {
            (Floating, x) | (x, Floating) => x,
            (High, High) => High,
            (Low, Low) => Low,
            _ => Conflict,
        }
    }

    /// one relaxation step. every conducting transistor joins its source and
    /// drain into one net (union-find), then every net takes a single value:
    ///
    ///   net touches drivers (VDD, GND, inputs)  -> their resolved value
    ///   net touches no driver                   -> keeps its stored charge
    ///
    /// Charge retention is what lets a cross-coupled latch hold its state.
    fn resolve_nets(&self) -> Vec<SignalState> {
        let n = self.signals.len();
        let mut parent: Vec<usize> = (0..n).collect();

        fn find(parent: &mut [usize], mut x: usize) -> usize {
            while parent[x] != x {
                parent[x] = parent[parent[x]]; // path halving
                x = parent[x];
            }
            x
        }

        for t in 0..self.kinds.len() {
            if self.conducting(t) {
                let a = find(&mut parent, self.sources[t].0);
                let b = find(&mut parent, self.drains[t].0);
                parent[a] = b;
            }
        }

        // per net root: the value forced by drivers, and the charge left on it
        let mut forced: Vec<Option<SignalState>> = vec![None; n];
        let mut charge: Vec<Option<SignalState>> = vec![None; n];
        for i in 0..n {
            let root = find(&mut parent, i);
            if let Some(state) = self.driven[i] {
                forced[root] = Some(Self::resolve(forced[root].unwrap_or(state), state));
            } else if matches!(self.signals[i], SignalState::High | SignalState::Low) {
                // charge sharing between nodes that disagree loses the value
                charge[root] =
                    Some(match charge[root] {
                             Some(prev) if prev != self.signals[i] => SignalState::Floating,
                             _ => self.signals[i],
                         });
            }
        }

        (0..n).map(|i| {
                  if let Some(state) = self.driven[i] {
                      return state; // drives always win on the driven node itself
                  }
                  let root = find(&mut parent, i);
                  forced[root].or(charge[root])
                              .unwrap_or(SignalState::Floating)
              })
              .collect()
    }

    /// relaxes the circuit to a fixed point. each step lets a change ripple
    /// through one more CMOS stage, so a combinational circuit settles in at
    /// most (depth + 1) steps; a circuit that is still changing at the cap is
    /// oscillating and is reported through `settled`.
    pub fn propagate(&mut self) {
        for i in 0..self.signals.len() {
            if let Some(state) = self.driven[i] {
                self.signals[i] = state;
            }
        }

        let max_iterations = self.kinds.len() + 2;
        self.settled = false;
        for _ in 0..max_iterations {
            let next = self.resolve_nets();
            if next == self.signals {
                self.settled = true;
                break;
            }
            self.signals = next;
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
    fn stage(&self, kind: GateKind, output: SignalId, first: usize) -> GateDescriptor {
        GateDescriptor { kind,
                         output,
                         body: GateBody::Stage((first..self.kinds.len()).collect()) }
    }

    pub fn nand(&mut self, a: SignalId, b: SignalId) -> GateDescriptor {
        let out = self.add_signal(None);
        let mid = self.add_signal(None);
        let first = self.kinds.len();
        self.add_transistor(TransistorKind::PMOS, a, self.vdd(), out);
        self.add_transistor(TransistorKind::PMOS, b, self.vdd(), out);
        self.add_transistor(TransistorKind::NMOS, a, mid, out);
        self.add_transistor(TransistorKind::NMOS, b, self.gnd(), mid);
        self.stage(GateKind::NAND, out, first)
    }

    pub fn not(&mut self, a: SignalId) -> GateDescriptor {
        let out = self.add_signal(None);
        let first = self.kinds.len();
        self.add_transistor(TransistorKind::PMOS, a, self.vdd(), out);
        self.add_transistor(TransistorKind::NMOS, a, self.gnd(), out);
        self.stage(GateKind::NOT, out, first)
    }

    pub fn and(&mut self, a: SignalId, b: SignalId) -> GateDescriptor {
        let nand = self.nand(a, b);
        let not = self.not(nand.output);
        GateDescriptor { kind: GateKind::AND,
                         output: not.output,
                         body: GateBody::Composite(vec![nand, not]) }
    }

    pub fn nor(&mut self, a: SignalId, b: SignalId) -> GateDescriptor {
        let out = self.add_signal(None);
        let mid = self.add_signal(None);
        let first = self.kinds.len();
        self.add_transistor(TransistorKind::PMOS, a, self.vdd(), mid);
        self.add_transistor(TransistorKind::PMOS, b, mid, out);
        self.add_transistor(TransistorKind::NMOS, a, self.gnd(), out);
        self.add_transistor(TransistorKind::NMOS, b, self.gnd(), out);
        self.stage(GateKind::NOR, out, first)
    }

    pub fn or(&mut self, a: SignalId, b: SignalId, small: bool) -> GateDescriptor {
        if small {
            let nor = self.nor(a, b);
            let not = self.not(nor.output);
            GateDescriptor { kind: GateKind::OR,
                             output: not.output,
                             body: GateBody::Composite(vec![nor, not]) }
        } else {
            let not_a = self.not(a);
            let not_b = self.not(b);
            let nand = self.nand(not_a.output, not_b.output);
            GateDescriptor { kind: GateKind::OR,
                             output: nand.output,
                             body: GateBody::Composite(vec![not_a, not_b, nand]) }
        }
    }

    pub fn xor(&mut self, a: SignalId, b: SignalId) -> GateDescriptor {
        let s1 = self.nand(a, b);
        let s2 = self.nand(a, s1.output);
        let s3 = self.nand(b, s1.output);
        let xor = self.nand(s2.output, s3.output);
        GateDescriptor { kind: GateKind::XOR,
                         output: xor.output,
                         body: GateBody::Composite(vec![s1, s2, s3, xor]) }
    }

    pub fn xnor(&mut self, a: SignalId, b: SignalId) -> GateDescriptor {
        let xor = self.xor(a, b);
        let xnor = self.not(xor.output);
        GateDescriptor { kind: GateKind::XNOR,
                         output: xnor.output,
                         body: GateBody::Composite(vec![xor, xnor]) }
    }
}
