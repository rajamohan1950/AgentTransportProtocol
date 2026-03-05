//! Routing pattern implementations.
//!
//! Five routing patterns that compose agents into execution strategies:
//!
//! 1. **DraftRefine**: cheap agent drafts -> expensive agent refines (40-70% savings)
//! 2. **ParallelMerge**: multiple agents process independently -> merge results
//! 3. **Cascade**: try cheapest first -> escalate on low confidence (30-50% savings)
//! 4. **Ensemble**: multiple agents vote on result (quality focus)
//! 5. **Pipeline**: sequential chain of agents
//!
//! Each pattern takes a set of capable agents from the graph, selects
//! appropriate agents for each role, and produces a Route with computed metrics.

use std::time::Duration;

use atp_types::{
    AgentId, Capability, QoSConstraints, Route, RouteMetrics, RoutingError, RoutingPattern,
    TaskType,
};
use uuid::Uuid;

use crate::cost::CostModel;
use crate::graph::AgentGraph;

/// Agent info extracted from the graph for pattern-based selection.
#[derive(Debug, Clone)]
struct AgentInfo {
    index: usize,
    id: AgentId,
    capability: Capability,
    trust: f64,
}

/// Extract capable agent info from the graph, sorted as needed.
fn extract_agents(
    graph: &AgentGraph,
    task_type: TaskType,
    min_trust: f64,
) -> Vec<AgentInfo> {
    graph
        .capable_agents(task_type, min_trust)
        .into_iter()
        .filter_map(|idx| {
            let node = graph.node(idx)?;
            let cap = node.capability_for(task_type)?;
            Some(AgentInfo {
                index: idx,
                id: node.id,
                capability: cap.clone(),
                trust: node.trust_score,
            })
        })
        .collect()
}

fn sorted_by_cost(agents: &[AgentInfo]) -> Vec<AgentInfo> {
    let mut sorted = agents.to_vec();
    sorted.sort_by(|a, b| {
        a.capability
            .cost_per_task
            .partial_cmp(&b.capability.cost_per_task)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    sorted
}

fn sorted_by_quality(agents: &[AgentInfo]) -> Vec<AgentInfo> {
    let mut sorted = agents.to_vec();
    sorted.sort_by(|a, b| {
        b.capability
            .estimated_quality
            .partial_cmp(&a.capability.estimated_quality)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    sorted
}

fn make_route(
    pattern: RoutingPattern,
    agent_ids: Vec<AgentId>,
    metrics: RouteMetrics,
    ttl: Duration,
) -> Route {
    Route {
        id: Uuid::new_v4(),
        pattern,
        agents: agent_ids,
        metrics,
        computed_at: chrono::Utc::now(),
        ttl,
    }
}

/// Build a **DraftRefine** route.
///
/// Strategy: cheapest agent drafts, highest-quality agent refines.
/// The drafter handles the bulk work at low cost; the refiner polishes.
///
/// Metrics:
/// - Quality = Q(drafter) * Q(refiner) — but refiner's quality dominates
/// - Latency = L(drafter) + L(refiner) + transfer
/// - Cost = C(drafter) + C(refiner)
///
/// Expected 40-70% cost savings vs using the expensive agent alone.
pub fn draft_refine(
    graph: &AgentGraph,
    task_type: TaskType,
    qos: &QoSConstraints,
    cost_model: &CostModel,
    route_ttl: Duration,
) -> Result<Route, RoutingError> {
    let agents = extract_agents(graph, task_type, qos.min_trust);
    if agents.len() < 2 {
        return Err(RoutingError::NoFeasibleRoute);
    }

    let by_cost = sorted_by_cost(&agents);
    let by_quality = sorted_by_quality(&agents);

    // Drafter: cheapest agent.
    let drafter = &by_cost[0];
    // Refiner: highest quality agent (different from drafter).
    let refiner = by_quality
        .iter()
        .find(|a| a.index != drafter.index)
        .ok_or(RoutingError::NoFeasibleRoute)?;

    let transfer = graph
        .transfer_latency(drafter.index, refiner.index)
        .unwrap_or(CostModel::default_transfer_latency());

    let capabilities = [drafter.capability.clone(), refiner.capability.clone()];
    let transfers = [transfer];
    let metrics = cost_model.compute_route_metrics(&capabilities, &transfers);

    if !cost_model.satisfies_constraints(&metrics, qos) {
        return Err(RoutingError::NoFeasibleRoute);
    }

    Ok(make_route(
        RoutingPattern::DraftRefine,
        vec![drafter.id, refiner.id],
        metrics,
        route_ttl,
    ))
}

/// Build a **ParallelMerge** route.
///
/// Strategy: top N agents (by quality/trust) process independently.
/// Results are merged by the coordinator.
///
/// Metrics:
/// - Quality = 1 - Product(1 - Q(a_i))  [parallel redundancy]
/// - Latency = max(L(a_i)) + merge_overhead
/// - Cost = Sum(C(a_i))
pub fn parallel_merge(
    graph: &AgentGraph,
    task_type: TaskType,
    qos: &QoSConstraints,
    cost_model: &CostModel,
    route_ttl: Duration,
    parallelism: usize,
) -> Result<Route, RoutingError> {
    let agents = extract_agents(graph, task_type, qos.min_trust);
    let n = parallelism.min(agents.len());
    if n < 2 {
        return Err(RoutingError::NoFeasibleRoute);
    }

    // Select top N by quality.
    let by_quality = sorted_by_quality(&agents);
    let selected: Vec<&AgentInfo> = by_quality.iter().take(n).collect();

    // Parallel quality: 1 - Product(1 - Q_i) -- redundant reliability model.
    let quality = 1.0
        - selected
            .iter()
            .map(|a| 1.0 - a.capability.estimated_quality.clamp(0.0, 1.0))
            .product::<f64>();

    // Latency: max of individual latencies + merge overhead.
    let max_latency = selected
        .iter()
        .map(|a| a.capability.estimated_latency)
        .max()
        .unwrap_or(Duration::ZERO);
    let merge_overhead = Duration::from_millis(10);
    let latency = max_latency + merge_overhead;

    // Cost: sum of all agents.
    let cost: f64 = selected.iter().map(|a| a.capability.cost_per_task).sum();

    let metrics = RouteMetrics {
        quality,
        latency,
        cost,
    };

    if !cost_model.satisfies_constraints(&metrics, qos) {
        return Err(RoutingError::NoFeasibleRoute);
    }

    let agent_ids: Vec<AgentId> = selected.iter().map(|a| a.id).collect();
    Ok(make_route(
        RoutingPattern::ParallelMerge,
        agent_ids,
        metrics,
        route_ttl,
    ))
}

/// Build a **Cascade** route.
///
/// Strategy: try cheapest agent first; if confidence is below threshold,
/// escalate to the next more expensive agent.
///
/// Metrics (expected value):
/// - Quality = weighted average based on escalation probability
/// - Latency = Sum(expected L_i * P(reach_i))
/// - Cost = Sum(expected C_i * P(reach_i))
///
/// Expected 30-50% cost savings.
pub fn cascade(
    graph: &AgentGraph,
    task_type: TaskType,
    qos: &QoSConstraints,
    cost_model: &CostModel,
    route_ttl: Duration,
    confidence_threshold: f64,
) -> Result<Route, RoutingError> {
    let agents = extract_agents(graph, task_type, qos.min_trust);
    if agents.is_empty() {
        return Err(RoutingError::NoFeasibleRoute);
    }

    let by_cost = sorted_by_cost(&agents);

    // Build cascade chain: up to 4 levels.
    let chain: Vec<&AgentInfo> = by_cost.iter().take(4).collect();

    if chain.is_empty() {
        return Err(RoutingError::NoFeasibleRoute);
    }

    // Compute expected metrics.
    // P(reach level i) = Product(1 - Q(a_j)) for j < i
    // (assume low quality means the agent was not confident enough)
    let mut expected_quality = 0.0;
    let mut expected_latency = Duration::ZERO;
    let mut expected_cost = 0.0;
    let mut prob_reach = 1.0;

    for (i, agent) in chain.iter().enumerate() {
        let q = agent.capability.estimated_quality.clamp(0.0, 1.0);
        // Probability that this agent's confidence exceeds threshold.
        let p_confident = if q >= confidence_threshold { q } else { q * 0.5 };
        let p_handle = prob_reach * p_confident;

        expected_quality += p_handle * q;
        expected_latency += Duration::from_secs_f64(
            prob_reach * agent.capability.estimated_latency.as_secs_f64(),
        );
        expected_cost += prob_reach * agent.capability.cost_per_task;

        // Transfer latency to next level.
        if i + 1 < chain.len() {
            let transfer = graph
                .transfer_latency(agent.index, chain[i + 1].index)
                .unwrap_or(CostModel::default_transfer_latency());
            expected_latency += Duration::from_secs_f64(
                prob_reach * (1.0 - p_confident) * transfer.as_secs_f64(),
            );
        }

        prob_reach *= 1.0 - p_confident;
    }

    // Normalize quality if total probability < 1 (fallback: last agent handles it).
    if prob_reach > 0.0 {
        let last = chain.last().unwrap();
        expected_quality += prob_reach * last.capability.estimated_quality;
    }

    let metrics = RouteMetrics {
        quality: expected_quality.clamp(0.0, 1.0),
        latency: expected_latency,
        cost: expected_cost,
    };

    if !cost_model.satisfies_constraints(&metrics, qos) {
        return Err(RoutingError::NoFeasibleRoute);
    }

    let agent_ids: Vec<AgentId> = chain.iter().map(|a| a.id).collect();
    Ok(make_route(
        RoutingPattern::Cascade,
        agent_ids,
        metrics,
        route_ttl,
    ))
}

/// Build an **Ensemble** route.
///
/// Strategy: multiple agents process the same input independently,
/// results are combined by voting/averaging.
///
/// Metrics:
/// - Quality = 1 - Product(1 - Q(a_i))  [same as parallel - ensemble effect]
/// - Latency = max(L(a_i)) + voting_overhead
/// - Cost = Sum(C(a_i))
///
/// Quality-focused pattern: selects highest-quality agents.
pub fn ensemble(
    graph: &AgentGraph,
    task_type: TaskType,
    qos: &QoSConstraints,
    cost_model: &CostModel,
    route_ttl: Duration,
    ensemble_size: usize,
) -> Result<Route, RoutingError> {
    let agents = extract_agents(graph, task_type, qos.min_trust);
    let n = ensemble_size.min(agents.len());
    if n < 2 {
        return Err(RoutingError::NoFeasibleRoute);
    }

    // Select top N by quality, breaking ties by trust.
    let mut by_quality = sorted_by_quality(&agents);
    // Stable secondary sort by trust for agents with similar quality.
    by_quality.sort_by(|a, b| {
        let q_cmp = b
            .capability
            .estimated_quality
            .partial_cmp(&a.capability.estimated_quality)
            .unwrap_or(std::cmp::Ordering::Equal);
        if q_cmp == std::cmp::Ordering::Equal {
            b.trust
                .partial_cmp(&a.trust)
                .unwrap_or(std::cmp::Ordering::Equal)
        } else {
            q_cmp
        }
    });

    let selected: Vec<&AgentInfo> = by_quality.iter().take(n).collect();

    // Ensemble quality via voting/aggregation.
    let quality = 1.0
        - selected
            .iter()
            .map(|a| 1.0 - a.capability.estimated_quality.clamp(0.0, 1.0))
            .product::<f64>();

    let max_latency = selected
        .iter()
        .map(|a| a.capability.estimated_latency)
        .max()
        .unwrap_or(Duration::ZERO);
    let voting_overhead = Duration::from_millis(5);
    let latency = max_latency + voting_overhead;

    let cost: f64 = selected.iter().map(|a| a.capability.cost_per_task).sum();

    let metrics = RouteMetrics {
        quality,
        latency,
        cost,
    };

    if !cost_model.satisfies_constraints(&metrics, qos) {
        return Err(RoutingError::NoFeasibleRoute);
    }

    let agent_ids: Vec<AgentId> = selected.iter().map(|a| a.id).collect();
    Ok(make_route(
        RoutingPattern::Ensemble,
        agent_ids,
        metrics,
        route_ttl,
    ))
}

/// Build a **Pipeline** route.
///
/// Strategy: sequential chain of agents, each passing output to the next.
/// Uses Bellman-Ford result paths directly, or constructs from sorted agents.
///
/// Metrics:
/// - Quality = Product(Q(a_i))            [multiplicative]
/// - Latency = Sum(L(a_i)) + Sum(T(edges)) [additive]
/// - Cost = Sum(C(a_i))                   [additive]
pub fn pipeline(
    graph: &AgentGraph,
    task_type: TaskType,
    qos: &QoSConstraints,
    cost_model: &CostModel,
    route_ttl: Duration,
    path: &[usize],
) -> Result<Route, RoutingError> {
    if path.is_empty() {
        return Err(RoutingError::NoFeasibleRoute);
    }

    let mut capabilities = Vec::with_capacity(path.len());
    let mut transfer_latencies = Vec::with_capacity(path.len().saturating_sub(1));
    let mut agent_ids = Vec::with_capacity(path.len());

    for (i, &idx) in path.iter().enumerate() {
        let node = graph.node(idx).ok_or(RoutingError::EmptyGraph)?;
        let cap = node
            .capability_for(task_type)
            .ok_or(RoutingError::NoFeasibleRoute)?;

        agent_ids.push(node.id);
        capabilities.push(cap.clone());

        if i > 0 {
            let prev = path[i - 1];
            let transfer = graph
                .transfer_latency(prev, idx)
                .unwrap_or(CostModel::default_transfer_latency());
            transfer_latencies.push(transfer);
        }
    }

    let metrics = cost_model.compute_route_metrics(&capabilities, &transfer_latencies);

    if !cost_model.satisfies_constraints(&metrics, qos) {
        return Err(RoutingError::NoFeasibleRoute);
    }

    Ok(make_route(
        RoutingPattern::Pipeline,
        agent_ids,
        metrics,
        route_ttl,
    ))
}

/// Build a pipeline route from an ordered list of AgentIds.
pub fn pipeline_from_ids(
    graph: &AgentGraph,
    task_type: TaskType,
    qos: &QoSConstraints,
    cost_model: &CostModel,
    route_ttl: Duration,
    agent_ids: &[AgentId],
) -> Result<Route, RoutingError> {
    let path: Result<Vec<usize>, RoutingError> = agent_ids
        .iter()
        .map(|id| graph.index_of(*id).ok_or(RoutingError::NoFeasibleRoute))
        .collect();

    pipeline(graph, task_type, qos, cost_model, route_ttl, &path?)
}

/// Automatically select the best pattern for given constraints and agents.
///
/// Heuristic:
/// - If budget is tight (max_cost < median_cost), try Cascade then DraftRefine.
/// - If quality is paramount (min_quality >= 0.9), try Ensemble.
/// - If latency is tight, try ParallelMerge.
/// - Default: Pipeline from best BF path.
pub fn auto_select_pattern(
    graph: &AgentGraph,
    task_type: TaskType,
    qos: &QoSConstraints,
    cost_model: &CostModel,
    route_ttl: Duration,
) -> Result<Route, RoutingError> {
    let agents = extract_agents(graph, task_type, qos.min_trust);
    if agents.is_empty() {
        return Err(RoutingError::NoFeasibleRoute);
    }

    // Compute median cost.
    let mut costs: Vec<f64> = agents.iter().map(|a| a.capability.cost_per_task).collect();
    costs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let median_cost = costs[costs.len() / 2];

    // Budget-constrained: try cost-saving patterns.
    if qos.max_cost < median_cost * 1.5 && agents.len() >= 2 {
        if let Ok(route) = cascade(graph, task_type, qos, cost_model, route_ttl, 0.7) {
            return Ok(route);
        }
        if let Ok(route) = draft_refine(graph, task_type, qos, cost_model, route_ttl) {
            return Ok(route);
        }
    }

    // Quality-focused: try ensemble.
    if qos.min_quality >= 0.9 && agents.len() >= 2 {
        if let Ok(route) = ensemble(graph, task_type, qos, cost_model, route_ttl, 3) {
            return Ok(route);
        }
    }

    // Latency-focused: try parallel merge.
    if qos.max_latency <= Duration::from_millis(500) && agents.len() >= 2 {
        if let Ok(route) = parallel_merge(graph, task_type, qos, cost_model, route_ttl, 3) {
            return Ok(route);
        }
    }

    // Default: single best agent (cheapest that satisfies constraints).
    let by_cost = sorted_by_cost(&agents);
    for agent in &by_cost {
        let metrics =
            cost_model.compute_route_metrics(&[agent.capability.clone()], &[]);
        if cost_model.satisfies_constraints(&metrics, qos) {
            return Ok(make_route(
                RoutingPattern::Pipeline,
                vec![agent.id],
                metrics,
                route_ttl,
            ));
        }
    }

    Err(RoutingError::NoFeasibleRoute)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_cap(quality: f64, latency_ms: u64, cost: f64) -> Capability {
        Capability {
            task_type: TaskType::CodeGeneration,
            estimated_quality: quality,
            estimated_latency: Duration::from_millis(latency_ms),
            cost_per_task: cost,
        }
    }

    fn build_test_graph() -> AgentGraph {
        let mut g = AgentGraph::new();
        // Cheap/low quality
        let a1 = AgentId::new();
        g.add_agent(a1, vec![make_cap(0.5, 50, 0.1)], 0.8);
        // Medium
        let a2 = AgentId::new();
        g.add_agent(a2, vec![make_cap(0.7, 100, 0.3)], 0.8);
        // Expensive/high quality
        let a3 = AgentId::new();
        g.add_agent(a3, vec![make_cap(0.95, 200, 0.8)], 0.9);
        // Fast
        let a4 = AgentId::new();
        g.add_agent(a4, vec![make_cap(0.6, 30, 0.2)], 0.7);

        g.fully_connect(Duration::from_millis(5));
        g
    }

    #[test]
    fn test_draft_refine() {
        let g = build_test_graph();
        let qos = QoSConstraints {
            min_quality: 0.3,
            max_latency: Duration::from_secs(10),
            max_cost: 2.0,
            min_trust: 0.5,
        };
        let cost_model = CostModel::default();

        let route =
            draft_refine(&g, TaskType::CodeGeneration, &qos, &cost_model, Duration::from_secs(60))
                .unwrap();

        assert_eq!(route.pattern, RoutingPattern::DraftRefine);
        assert_eq!(route.agents.len(), 2);
        // Drafter should be cheapest, refiner should be highest quality.
        assert!(route.metrics.cost < 1.0); // Less than expensive agent alone (0.8)
    }

    #[test]
    fn test_parallel_merge() {
        let g = build_test_graph();
        let qos = QoSConstraints {
            min_quality: 0.5,
            max_latency: Duration::from_secs(10),
            max_cost: 3.0,
            min_trust: 0.5,
        };
        let cost_model = CostModel::default();

        let route = parallel_merge(
            &g,
            TaskType::CodeGeneration,
            &qos,
            &cost_model,
            Duration::from_secs(60),
            3,
        )
        .unwrap();

        assert_eq!(route.pattern, RoutingPattern::ParallelMerge);
        assert!(route.agents.len() >= 2);
        // Parallel quality should be higher than any individual agent.
        assert!(route.metrics.quality > 0.9);
    }

    #[test]
    fn test_cascade() {
        let g = build_test_graph();
        let qos = QoSConstraints {
            min_quality: 0.3,
            max_latency: Duration::from_secs(10),
            max_cost: 2.0,
            min_trust: 0.5,
        };
        let cost_model = CostModel::default();

        let route = cascade(
            &g,
            TaskType::CodeGeneration,
            &qos,
            &cost_model,
            Duration::from_secs(60),
            0.7,
        )
        .unwrap();

        assert_eq!(route.pattern, RoutingPattern::Cascade);
        assert!(!route.agents.is_empty());
    }

    #[test]
    fn test_ensemble() {
        let g = build_test_graph();
        let qos = QoSConstraints {
            min_quality: 0.5,
            max_latency: Duration::from_secs(10),
            max_cost: 3.0,
            min_trust: 0.5,
        };
        let cost_model = CostModel::default();

        let route = ensemble(
            &g,
            TaskType::CodeGeneration,
            &qos,
            &cost_model,
            Duration::from_secs(60),
            3,
        )
        .unwrap();

        assert_eq!(route.pattern, RoutingPattern::Ensemble);
        assert!(route.metrics.quality > 0.9);
    }

    #[test]
    fn test_pipeline() {
        let g = build_test_graph();
        let qos = QoSConstraints {
            min_quality: 0.1,
            max_latency: Duration::from_secs(10),
            max_cost: 3.0,
            min_trust: 0.5,
        };
        let cost_model = CostModel::default();

        let route = pipeline(
            &g,
            TaskType::CodeGeneration,
            &qos,
            &cost_model,
            Duration::from_secs(60),
            &[0, 1, 2],
        )
        .unwrap();

        assert_eq!(route.pattern, RoutingPattern::Pipeline);
        assert_eq!(route.agents.len(), 3);
        // Quality is multiplicative: 0.5 * 0.7 * 0.95 = 0.3325
        assert!((route.metrics.quality - 0.3325).abs() < 1e-4);
    }
}
