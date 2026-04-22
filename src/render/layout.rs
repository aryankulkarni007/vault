use std::collections::{HashMap, HashSet, VecDeque};

use crate::sim::SignalGraph;

pub fn find_clusters(graph: &SignalGraph, transistors: &[usize]) -> Vec<Vec<usize>> {
    // signal -> which transistors touch it
    let mut sig_to_trans: HashMap<usize, Vec<usize>> = HashMap::new();
    for &t in transistors {
        for sig in [graph.gates[t], graph.sources[t], graph.drains[t]] {
            if sig.0 > 1 {
                sig_to_trans.entry(sig.0).or_default().push(t);
            }
        }
    }
    let mut visited: HashSet<usize> = HashSet::new();
    let mut clusters: Vec<Vec<usize>> = Vec::new();

    for &t in transistors {
        if visited.contains(&t) {
            continue;
        }
        let mut cluster = Vec::new();
        let mut queue = VecDeque::from([t]);
        while let Some(cur) = queue.pop_front() {
            if !visited.insert(cur) {
                continue;
            }
            cluster.push(cur);
            for sig in [graph.gates[cur], graph.sources[cur], graph.drains[cur]] {
                if sig.0 > 1 {
                    for &neighbor in sig_to_trans.get(&sig.0).unwrap_or(&vec![]) {
                        if !visited.contains(&neighbor) {
                            queue.push_back(neighbor);
                        }
                    }
                }
            }
        }
        clusters.push(cluster);
    }
    clusters
}
