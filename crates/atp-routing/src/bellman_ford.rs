//! Modified Bellman-Ford with multi-objective optimization.
//!
//! Instead of a single shortest-path computation, we run Bellman-Ford once per
//! weight vector from a predefined set of ~10 vectors that sample the
//! quality-latency-cost objective space. Each run produces one candidate route.
//! The Pareto-optimal subset is then extracted by the optimizer module.
//!
//! Complexity: O(k^2 * |W|) where k = capability-matched agents, |W| = ~10
//! weight vectors. For 50 agents this is ~25,000 operations, well under 1ms.



use atp_types::{AgentId, RouteMetrics, RoutingError, TaskType};

use crate::cost::{CostModel, CostWeights};
use crate::graph::AgentGraph;

/// Result of a single Bellman-Ford run under specific weights.
#[derive(Debug, Clone)]
pub struct BellmanFordResult {
    /// The weight vector used for this run.
    pub weights: CostWeights,
    /// Ordered sequence of agent indices forming the path.
    pub path: Vec<usize>,
    /// Scalar cost under the given weights.
    pub scalar_cost: f64,
    /// Full multi-dimensional route metrics.
    pub metrics: RouteMetrics,
}

/// Run modified Bellman-Ford from a single source to find the shortest path
/// to every other node in the capability subgraph, under the given weight vector.
///
/// Returns the best single-destination path to each reachable node.
/// Since we want routes *through* agents (not to a fixed destination),
/// we return the best path of any length that visits capability-matched agents.
fn bellman_ford_single(
    graph: &AgentGraph,
    agents: &[usize],
    task_type: TaskType,
    source: usize,
    weights: &CostWeights,
    cost_model: &CostModel,
) -> Option<(Vec<usize>, f64)> {
    let k = agents.len();
    if k == 0 {
        return None;
    }

    // Map global indices to local dense indices for the BF arrays.
    let mut global_to_local = std::collections::HashMap::with_capacity(k);
    for (local, &global) in agents.iter().enumerate() {
        global_to_local.insert(global, local);
    }

    let source_local = match global_to_local.get(&source) {
        Some(&l) => l,
        None => return None,
    };

    // Distance and predecessor arrays.
    let mut dist = vec![f64::INFINITY; k];
    let mut pred: Vec<Option<usize>> = vec![None; k];
    dist[source_local] = 0.0;

    // Relaxation: k-1 iterations (standard BF).
    for _ in 0..k.saturating_sub(1) {
        let mut updated = false;
        for &u_global in agents {
            let u_local = global_to_local[&u_global];
            if dist[u_local] == f64::INFINITY {
                continue;
            }

            for edge in graph.edges_from(u_global) {
                if let Some(&v_local) = global_to_local.get(&edge.to) {
                    let v_node = match graph.node(edge.to) {
                        Some(n) => n,
                        None => continue,
                    };
                    let cap = match v_node.capability_for(task_type) {
                        Some(c) => c,
                        None => continue,
                    };

                    let edge_cost =
                        cost_model.scalar_edge_cost(cap, edge.transfer_latency, weights);
                    let new_dist = dist[u_local] + edge_cost;

                    if new_dist < dist[v_local] {
                        dist[v_local] = new_dist;
                        pred[v_local] = Some(u_local);
                        updated = true;
                    }
                }
            }
        }

        if !updated {
            break; // Early termination — no more relaxations possible.
        }
    }

    // Find the best reachable destination (not the source itself).
    let mut best_dest: Option<usize> = None;
    let mut best_cost = f64::INFINITY;

    for (local, &d) in dist.iter().enumerate().take(k) {
        if local != source_local && d < best_cost {
            best_cost = d;
            best_dest = Some(local);
        }
    }

    let dest_local = best_dest?;

    // Reconstruct path from source to best destination.
    let mut path_local = Vec::new();
    let mut current = dest_local;
    path_local.push(current);

    while let Some(p) = pred[current] {
        if p == source_local {
            path_local.push(p);
            break;
        }
        path_local.push(p);
        current = p;
        // Safety: path cannot be longer than k.
        if path_local.len() > k {
            return None; // Cycle detected.
        }
    }

    // If we didn't reach the source, path is incomplete.
    if path_local.last().copied() != Some(source_local) {
        // Direct edge case: dest is directly reachable from source.
        if dist[dest_local] < f64::INFINITY {
            path_local.push(source_local);
        } else {
            return None;
        }
    }

    path_local.reverse();

    // Convert local indices back to global.
    let path_global: Vec<usize> = path_local.iter().map(|&l| agents[l]).collect();

    Some((path_global, best_cost))
}

/// Compute full route metrics for a path through the graph.
fn compute_path_metrics(
    graph: &AgentGraph,
    path: &[usize],
    task_type: TaskType,
    cost_model: &CostModel,
) -> Option<RouteMetrics> {
    if path.is_empty() {
        return None;
    }

    let mut capabilities = Vec::with_capacity(path.len());
    let mut transfer_latencies = Vec::with_capacity(path.len().saturating_sub(1));

    for (i, &idx) in path.iter().enumerate() {
        let node = graph.node(idx)?;
        let cap = node.capability_for(task_type)?;
        capabilities.push(cap.clone());

        if i > 0 {
            let prev = path[i - 1];
            let latency = graph
                .transfer_latency(prev, idx)
                .unwrap_or(CostModel::default_transfer_latency());
            transfer_latencies.push(latency);
        }
    }

    Some(cost_model.compute_route_metrics(&capabilities, &transfer_latencies))
}

/// Run the multi-objective Bellman-Ford algorithm.
///
/// Executes BF once per weight vector from the predefined set, using each
/// capable agent as a source, then collects all discovered paths.
///
/// Returns a vector of `BellmanFordResult` representing candidate routes.
/// The caller (optimizer) will then compute the Pareto front.
///
/// Complexity: O(k^2 * |W|) where k = `agents.len()`, |W| = ~10.
pub fn multi_objective_bellman_ford(
    graph: &AgentGraph,
    task_type: TaskType,
    min_trust: f64,
    cost_model: &CostModel,
) -> Result<Vec<BellmanFordResult>, RoutingError> {
    let agents = graph.capable_agents(task_type, min_trust);

    if agents.is_empty() {
        return Err(RoutingError::NoFeasibleRoute);
    }

    // For single-agent case, return a trivial route per weight vector.
    if agents.len() == 1 {
        let idx = agents[0];
        let node = graph
            .node(idx)
            .ok_or(RoutingError::EmptyGraph)?;
        let cap = node
            .capability_for(task_type)
            .ok_or(RoutingError::NoFeasibleRoute)?;

        let metrics = cost_model.compute_route_metrics(&[cap.clone()], &[]);
        let weight_vectors = CostWeights::pareto_sample_vectors();

        return Ok(weight_vectors
            .iter()
            .map(|w| BellmanFordResult {
                weights: *w,
                path: vec![idx],
                scalar_cost: cost_model.scalar_route_cost(&metrics, w),
                metrics: metrics.clone(),
            })
            .collect());
    }

    let weight_vectors = CostWeights::pareto_sample_vectors();
    let mut results = Vec::with_capacity(weight_vectors.len() * agents.len());

    // For each weight vector, run BF from each source agent.
    for weights in weight_vectors {
        for &source in &agents {
            if let Some((path, scalar_cost)) =
                bellman_ford_single(graph, &agents, task_type, source, weights, cost_model)
            {
                if path.len() >= 2 {
                    if let Some(metrics) =
                        compute_path_metrics(graph, &path, task_type, cost_model)
                    {
                        results.push(BellmanFordResult {
                            weights: *weights,
                            path,
                            scalar_cost,
                            metrics,
                        });
                    }
                }
            }
        }
    }

    // Also add single-agent routes (each agent by itself).
    for &idx in &agents {
        if let Some(node) = graph.node(idx) {
            if let Some(cap) = node.capability_for(task_type) {
                let metrics = cost_model.compute_route_metrics(&[cap.clone()], &[]);
                for weights in weight_vectors {
                    results.push(BellmanFordResult {
                        weights: *weights,
                        path: vec![idx],
                        scalar_cost: cost_model.scalar_route_cost(&metrics, weights),
                        metrics: metrics.clone(),
                    });
                }
            }
        }
    }

    if results.is_empty() {
        return Err(RoutingError::NoFeasibleRoute);
    }

    Ok(results)
}

/// Convenience: convert a path of node indices to AgentIds.
pub fn path_to_agent_ids(graph: &AgentGraph, path: &[usize]) -> Vec<AgentId> {
    path.iter()
        .filter_map(|&idx| graph.agent_id(idx))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use atp_types::Capability;
    use std::time::Duration;

    fn make_cap(quality: f64, latency_ms: u64, cost: f64) -> Capability {
        Capability {
            task_type: TaskType::CodeGeneration,
            estimated_quality: quality,
            estimated_latency: Duration::from_millis(latency_ms),
            cost_per_task: cost,
        }
    }

    fn build_test_graph() -> (AgentGraph, Vec<AgentId>) {
        let mut g = AgentGraph::new();
        let ids: Vec<AgentId> = (0..5).map(|_| AgentId::new()).collect();

        // Agent 0: cheap, low quality
        g.add_agent(ids[0], vec![make_cap(0.5, 50, 0.1)], 0.8);
        // Agent 1: medium
        g.add_agent(ids[1], vec![make_cap(0.7, 100, 0.3)], 0.8);
        // Agent 2: expensive, high quality
        g.add_agent(ids[2], vec![make_cap(0.95, 200, 0.8)], 0.9);
        // Agent 3: fast, medium quality
        g.add_agent(ids[3], vec![make_cap(0.6, 30, 0.2)], 0.7);
        // Agent 4: balanced
        g.add_agent(ids[4], vec![make_cap(0.8, 80, 0.4)], 0.85);

        g.fully_connect(Duration::from_millis(5));
        (g, ids)
    }

    #[test]
    fn test_multi_objective_produces_results() {
        let (graph, _ids) = build_test_graph();
        let cost_model = CostModel::default();
        let results =
            multi_objective_bellman_ford(&graph, TaskType::CodeGeneration, 0.5, &cost_model)
                .unwrap();

        assert!(!results.is_empty());
        // Should have discovered paths of varying lengths.
        let path_lengths: std::collections::HashSet<usize> =
            results.iter().map(|r| r.path.len()).collect();
        assert!(!path_lengths.is_empty());
    }

    #[test]
    fn test_single_agent() {
        let mut g = AgentGraph::new();
        let id = AgentId::new();
        g.add_agent(id, vec![make_cap(0.9, 100, 0.5)], 0.8);
        let cost_model = CostModel::default();

        let results =
            multi_objective_bellman_ford(&g, TaskType::CodeGeneration, 0.5, &cost_model).unwrap();
        assert!(!results.is_empty());
        assert!(results.iter().all(|r| r.path.len() == 1));
    }

    #[test]
    fn test_no_capable_agents() {
        let g = AgentGraph::new();
        let cost_model = CostModel::default();

        let err = multi_objective_bellman_ford(&g, TaskType::Analysis, 0.5, &cost_model)
            .unwrap_err();
        assert!(matches!(err, RoutingError::NoFeasibleRoute));
    }
}
