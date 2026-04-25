use std::collections::VecDeque;

use crate::sim::transistor::GateDescriptor;

fn build_edges(gates: &[GateDescriptor]) -> Vec<(usize, usize)> {
    // build a map from output value to gate index for o(1) lookups
    let output_to_index: std::collections::HashMap<_, _> = gates
        .iter()
        .enumerate()
        .map(|(idx, gate)| (&gate.output, idx))
        .collect();

    // collect edges in a single pass
    gates
        .iter()
        .enumerate()
        .flat_map(|(j, gate_j)| {
            gate_j.inputs.iter().filter_map({
                let value = output_to_index.clone();
                move |input| value.get(input).map(|&i| (i, j))
            })
        })
        .collect()
}

fn kahn_layout(gates: &[GateDescriptor], edges: &[(usize, usize)]) -> Vec<(usize, usize)> {
    let n = gates.len();

    // build adjacency list and compute in-degrees
    let mut adj = vec![Vec::new(); n];
    let mut in_degree = vec![0; n];
    for &(src, dst) in edges {
        adj[src].push(dst);
        in_degree[dst] += 1;
    }

    let mut queue = VecDeque::new();
    for i in 0..n {
        if in_degree[i] == 0 {
            queue.push_back(i);
        }
    }

    // if graph has cycles, topo_order length < n -> handle
    let mut topo_order = Vec::with_capacity(n);
    while let Some(u) = queue.pop_front() {
        topo_order.push(u);
        for &v in &adj[u] {
            in_degree[v] -= 1;
            if in_degree[v] == 0 {
                queue.push_back(v);
            }
        }
    }

    // Assign layer = length of longest path from any source
    // Initialize with 0; then process nodes in topological order
    let mut layer = vec![0; n];
    for &u in &topo_order {
        for &v in &adj[u] {
            // Edge u -> v : v's layer should be at least layer[u] + 1
            if layer[v] <= layer[u] {
                layer[v] = layer[u] + 1;
            }
        }
    }

    // now group nodes by layer and assign an order within each layer
    // first, find max layer to know how many layers we have
    if gates.is_empty() {
        return vec![];
    }
    let max_layer = *layer.iter().max().unwrap();
    let mut layer_nodes: Vec<Vec<usize>> = vec![Vec::new(); max_layer + 1];
    for (node, &l) in layer.iter().enumerate() {
        layer_nodes[l].push(node);
    }

    // for each layer, sort nodes (e.g. by original index) to get a stable index
    let mut result = vec![(0, 0); n];
    (0..=max_layer).for_each(|l| {
        let nodes = &mut layer_nodes[l];
        nodes.sort();
        for (pos, &node) in nodes.iter().enumerate() {
            result[node] = (l, pos);
        }
    });
    result
}
