mod render;
mod sim;

use crate::sim::SignalGraph;

fn main() {
    // initialisation of the circuit
    let mut signal_graph = SignalGraph::new();
    let a = signal_graph.add_signal(Some("A"));
    let b = signal_graph.add_signal(Some("B"));
    let _out = signal_graph.nand(a, b);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::SignalState;

    #[test]
    fn nand() {
        let mut g = SignalGraph::new();
        let a = g.add_signal(Some("A"));
        let b = g.add_signal(Some("B"));
        let out = g.nand(a, b);

        // test case 1 (Low Low -> High)
        g.drive(a, Some(SignalState::Low));
        g.drive(b, Some(SignalState::Low));
        g.propagate();
        assert_eq!(g.signals[out.0], SignalState::High);

        // test case 2 (Low High -> High)
        g.drive(a, Some(SignalState::Low));
        g.drive(b, Some(SignalState::High));
        g.propagate();
        assert_eq!(g.signals[out.0], SignalState::High);

        // test case 3 (High Low -> High)
        g.drive(a, Some(SignalState::High));
        g.drive(b, Some(SignalState::Low));
        g.propagate();
        assert_eq!(g.signals[out.0], SignalState::High);

        // test case 4 (High High -> Low)
        g.drive(a, Some(SignalState::High));
        g.drive(b, Some(SignalState::High));
        g.propagate();
        assert_eq!(g.signals[out.0], SignalState::Low);
    }
}
