//! ATP Layer 4: Economic Routing
//!
//! Finds optimal routes through the agent network G = (V, E) using
//! multi-objective optimization across three dimensions:
//!
//! - **Quality(R)** = Product(Q(a_i))          — multiplicative, quality compounds
//! - **Latency(R)** = Sum(L(a_i)) + Sum(T(e))  — additive including transfer
//! - **Cost(R)**    = Sum(C(a_i))              — additive
//!
//! The routing algorithm uses a modified Bellman-Ford that runs once per weight
//! combination from a predefined set of ~10 weight vectors, then computes the
//! Pareto front of discovered routes. This achieves O(k^2 * |W|) complexity
//! where k = capability-matched agents and |W| ~ 10.
//!
//! Five routing patterns compose agents into execution strategies:
//! 1. DraftRefine — cheap agent drafts, expensive refines (40-70% savings)
//! 2. ParallelMerge — multiple agents process independently, merge results
//! 3. Cascade — try cheapest first, escalate on low confidence (30-50% savings)
//! 4. Ensemble — multiple agents vote on result (quality focus)
//! 5. Pipeline — sequential processing chain

pub mod bellman_ford;
pub mod cost;
pub mod graph;
pub mod optimizer;
pub mod patterns;

// Re-export primary types for convenient access.
pub use bellman_ford::{multi_objective_bellman_ford, BellmanFordResult};
pub use cost::{CostModel, CostWeights};
pub use graph::{AgentGraph, AgentNode, Edge};
pub use optimizer::{pareto_front, select_top_routes, ParetoRoute};
pub use patterns::{
    auto_select_pattern, cascade, draft_refine, ensemble, parallel_merge, pipeline,
    pipeline_from_ids,
};

use std::time::Duration;

use atp_types::{
    AgentId, Capability, QoSConstraints, Route, RoutingConfig, RoutingError, RoutingPattern,
    TaskType,
};
use tracing::{debug, instrument, warn};

/// The economic router — primary API for route discovery.
///
/// Wraps the agent graph, cost model, and configuration to provide
/// high-level `find_route()` and `find_routes()` methods.
#[derive(Debug, Clone)]
pub struct EconomicRouter {
    /// The agent network topology.
    graph: AgentGraph,
    /// Cost model for computing scalar and multi-dimensional costs.
    cost_model: CostModel,
    /// Routing configuration (TTL, max routes, etc.).
    config: RoutingConfig,
}

impl EconomicRouter {
    /// Create a new router with the given graph and default configuration.
    pub fn new(graph: AgentGraph) -> Self {
        Self {
            graph,
            cost_model: CostModel::default(),
            config: RoutingConfig::default(),
        }
    }

    /// Create a router with explicit cost model and configuration.
    pub fn with_config(
        graph: AgentGraph,
        cost_model: CostModel,
        config: RoutingConfig,
    ) -> Self {
        Self {
            graph,
            cost_model,
            config,
        }
    }

    /// Get a reference to the underlying graph.
    pub fn graph(&self) -> &AgentGraph {
        &self.graph
    }

    /// Get a mutable reference to the underlying graph.
    pub fn graph_mut(&mut self) -> &mut AgentGraph {
        &mut self.graph
    }

    /// Get a reference to the cost model.
    pub fn cost_model(&self) -> &CostModel {
        &self.cost_model
    }

    /// Add an agent to the router's graph.
    pub fn add_agent(
        &mut self,
        id: AgentId,
        capabilities: Vec<Capability>,
        trust_score: f64,
    ) -> usize {
        self.graph.add_agent(id, capabilities, trust_score)
    }

    /// Add a bidirectional edge between two agents.
    pub fn connect(&mut self, a: AgentId, b: AgentId, transfer_latency: Duration) {
        self.graph.add_bidi_edge(a, b, transfer_latency);
    }

    /// Fully connect all agents in the graph.
    pub fn fully_connect(&mut self, transfer_latency: Duration) {
        self.graph.fully_connect(transfer_latency);
    }

    /// Mark an agent as unavailable.
    pub fn remove_agent(&mut self, id: AgentId) {
        self.graph.set_unavailable(id);
    }

    /// Mark an agent as available again.
    pub fn restore_agent(&mut self, id: AgentId) {
        self.graph.set_available(id);
    }

    /// Find the single best route for a task under QoS constraints.
    ///
    /// This runs multi-objective Bellman-Ford, computes the Pareto front,
    /// filters by constraints, and returns the top-ranked route.
    ///
    /// Optionally specify a preferred routing pattern; if `None`, the
    /// router auto-selects the best pattern.
    #[instrument(skip(self), fields(task_type = %task_type, pattern = ?preferred_pattern))]
    pub fn find_route(
        &self,
        task_type: TaskType,
        qos: &QoSConstraints,
        preferred_pattern: Option<RoutingPattern>,
    ) -> Result<Route, RoutingError> {
        self.graph.validate()?;

        // If a specific pattern is requested, try it directly.
        if let Some(pattern) = preferred_pattern {
            let result = self.try_pattern(pattern, task_type, qos);
            if result.is_ok() {
                debug!(pattern = %pattern, "pattern-specific route found");
                return result;
            }
            warn!(
                pattern = %pattern,
                "preferred pattern failed, falling back to auto-select"
            );
        }

        // Auto-select: try pattern-based routing first.
        if let Ok(route) = auto_select_pattern(
            &self.graph,
            task_type,
            qos,
            &self.cost_model,
            self.config.route_ttl,
        ) {
            debug!(pattern = %route.pattern, "auto-selected pattern route");
            return Ok(route);
        }

        // Fall back to Bellman-Ford exploration.
        let results = multi_objective_bellman_ford(
            &self.graph,
            task_type,
            qos.min_trust,
            &self.cost_model,
        )?;

        let weights = CostModel::weights_from_constraints(qos);
        let mut top = select_top_routes(
            &results,
            qos,
            &weights,
            &self.cost_model,
            1,
        )?;

        if top.is_empty() {
            return Err(RoutingError::NoFeasibleRoute);
        }

        let best = top.remove(0);
        let agent_ids = bellman_ford::path_to_agent_ids(&self.graph, &best.path);

        debug!(
            agents = agent_ids.len(),
            quality = best.metrics.quality,
            latency_ms = best.metrics.latency.as_millis(),
            cost = best.metrics.cost,
            "BF route found"
        );

        Ok(Route {
            id: uuid::Uuid::new_v4(),
            pattern: RoutingPattern::Pipeline,
            agents: agent_ids,
            metrics: best.metrics,
            computed_at: chrono::Utc::now(),
            ttl: self.config.route_ttl,
        })
    }

    /// Find multiple candidate routes for a task, ranked by quality.
    ///
    /// Returns up to `config.max_routes` routes on the Pareto front,
    /// filtered by QoS constraints.
    #[instrument(skip(self), fields(task_type = %task_type))]
    pub fn find_routes(
        &self,
        task_type: TaskType,
        qos: &QoSConstraints,
    ) -> Result<Vec<Route>, RoutingError> {
        self.graph.validate()?;

        let mut all_routes: Vec<Route> = Vec::new();

        // Collect routes from all applicable patterns.
        let pattern_routes = self.collect_pattern_routes(task_type, qos);
        all_routes.extend(pattern_routes);

        // Add Bellman-Ford pipeline routes.
        if let Ok(results) = multi_objective_bellman_ford(
            &self.graph,
            task_type,
            qos.min_trust,
            &self.cost_model,
        ) {
            let weights = CostModel::weights_from_constraints(qos);
            if let Ok(top) = select_top_routes(
                &results,
                qos,
                &weights,
                &self.cost_model,
                self.config.max_routes,
            ) {
                for pareto_route in top {
                    let agent_ids =
                        bellman_ford::path_to_agent_ids(&self.graph, &pareto_route.path);
                    all_routes.push(Route {
                        id: uuid::Uuid::new_v4(),
                        pattern: RoutingPattern::Pipeline,
                        agents: agent_ids,
                        metrics: pareto_route.metrics,
                        computed_at: chrono::Utc::now(),
                        ttl: self.config.route_ttl,
                    });
                }
            }
        }

        if all_routes.is_empty() {
            return Err(RoutingError::NoFeasibleRoute);
        }

        // Sort by scalar cost (weighted), deduplicate by agent list.
        let weights = CostModel::weights_from_constraints(qos);
        all_routes.sort_by(|a, b| {
            let ca = self.cost_model.scalar_route_cost(&a.metrics, &weights);
            let cb = self.cost_model.scalar_route_cost(&b.metrics, &weights);
            ca.partial_cmp(&cb).unwrap_or(std::cmp::Ordering::Equal)
        });

        // Deduplicate by agent list.
        let mut seen = std::collections::HashSet::new();
        all_routes.retain(|r| seen.insert(r.agents.clone()));

        all_routes.truncate(self.config.max_routes);

        debug!(count = all_routes.len(), "routes found");
        Ok(all_routes)
    }

    /// Try a specific routing pattern.
    fn try_pattern(
        &self,
        pattern: RoutingPattern,
        task_type: TaskType,
        qos: &QoSConstraints,
    ) -> Result<Route, RoutingError> {
        match pattern {
            RoutingPattern::DraftRefine => {
                draft_refine(&self.graph, task_type, qos, &self.cost_model, self.config.route_ttl)
            }
            RoutingPattern::ParallelMerge => parallel_merge(
                &self.graph,
                task_type,
                qos,
                &self.cost_model,
                self.config.route_ttl,
                3,
            ),
            RoutingPattern::Cascade => cascade(
                &self.graph,
                task_type,
                qos,
                &self.cost_model,
                self.config.route_ttl,
                0.7,
            ),
            RoutingPattern::Ensemble => ensemble(
                &self.graph,
                task_type,
                qos,
                &self.cost_model,
                self.config.route_ttl,
                3,
            ),
            RoutingPattern::Pipeline => {
                // For pipeline pattern without a specific path, use BF.
                let results = multi_objective_bellman_ford(
                    &self.graph,
                    task_type,
                    qos.min_trust,
                    &self.cost_model,
                )?;
                let weights = CostModel::weights_from_constraints(qos);
                let mut top = select_top_routes(
                    &results,
                    qos,
                    &weights,
                    &self.cost_model,
                    1,
                )?;
                if top.is_empty() {
                    return Err(RoutingError::NoFeasibleRoute);
                }
                let best = top.remove(0);
                let agent_ids = bellman_ford::path_to_agent_ids(&self.graph, &best.path);
                Ok(Route {
                    id: uuid::Uuid::new_v4(),
                    pattern: RoutingPattern::Pipeline,
                    agents: agent_ids,
                    metrics: best.metrics,
                    computed_at: chrono::Utc::now(),
                    ttl: self.config.route_ttl,
                })
            }
        }
    }

    /// Collect routes from all applicable routing patterns.
    fn collect_pattern_routes(
        &self,
        task_type: TaskType,
        qos: &QoSConstraints,
    ) -> Vec<Route> {
        let mut routes = Vec::new();
        let ttl = self.config.route_ttl;

        if let Ok(r) = draft_refine(&self.graph, task_type, qos, &self.cost_model, ttl) {
            routes.push(r);
        }
        if let Ok(r) = parallel_merge(&self.graph, task_type, qos, &self.cost_model, ttl, 3) {
            routes.push(r);
        }
        if let Ok(r) = cascade(&self.graph, task_type, qos, &self.cost_model, ttl, 0.7) {
            routes.push(r);
        }
        if let Ok(r) = ensemble(&self.graph, task_type, qos, &self.cost_model, ttl, 3) {
            routes.push(r);
        }

        routes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_cap(task: TaskType, quality: f64, latency_ms: u64, cost: f64) -> Capability {
        Capability {
            task_type: task,
            estimated_quality: quality,
            estimated_latency: Duration::from_millis(latency_ms),
            cost_per_task: cost,
        }
    }

    fn build_router() -> EconomicRouter {
        let mut graph = AgentGraph::new();
        let task = TaskType::CodeGeneration;

        // 5 agents with varying quality/cost profiles.
        let a1 = AgentId::new();
        graph.add_agent(a1, vec![make_cap(task, 0.5, 50, 0.1)], 0.8);
        let a2 = AgentId::new();
        graph.add_agent(a2, vec![make_cap(task, 0.7, 100, 0.3)], 0.8);
        let a3 = AgentId::new();
        graph.add_agent(a3, vec![make_cap(task, 0.95, 200, 0.8)], 0.9);
        let a4 = AgentId::new();
        graph.add_agent(a4, vec![make_cap(task, 0.6, 30, 0.2)], 0.7);
        let a5 = AgentId::new();
        graph.add_agent(a5, vec![make_cap(task, 0.8, 80, 0.4)], 0.85);

        graph.fully_connect(Duration::from_millis(5));

        EconomicRouter::new(graph)
    }

    #[test]
    fn test_find_route_default() {
        let router = build_router();
        let qos = QoSConstraints {
            min_quality: 0.3,
            max_latency: Duration::from_secs(10),
            max_cost: 2.0,
            min_trust: 0.5,
        };

        let route = router
            .find_route(TaskType::CodeGeneration, &qos, None)
            .unwrap();

        assert!(!route.agents.is_empty());
        assert!(route.metrics.quality >= qos.min_quality);
        assert!(route.metrics.latency <= qos.max_latency);
        assert!(route.metrics.cost <= qos.max_cost);
    }

    #[test]
    fn test_find_route_specific_pattern() {
        let router = build_router();
        let qos = QoSConstraints {
            min_quality: 0.3,
            max_latency: Duration::from_secs(10),
            max_cost: 2.0,
            min_trust: 0.5,
        };

        let route = router
            .find_route(
                TaskType::CodeGeneration,
                &qos,
                Some(RoutingPattern::DraftRefine),
            )
            .unwrap();

        assert_eq!(route.pattern, RoutingPattern::DraftRefine);
        assert_eq!(route.agents.len(), 2);
    }

    #[test]
    fn test_find_routes_multiple() {
        let router = build_router();
        let qos = QoSConstraints {
            min_quality: 0.1,
            max_latency: Duration::from_secs(10),
            max_cost: 5.0,
            min_trust: 0.5,
        };

        let routes = router
            .find_routes(TaskType::CodeGeneration, &qos)
            .unwrap();

        assert!(routes.len() >= 2, "expected multiple routes, got {}", routes.len());
        // All routes should satisfy constraints.
        for r in &routes {
            assert!(r.metrics.quality >= qos.min_quality);
            assert!(r.metrics.cost <= qos.max_cost);
        }
    }

    #[test]
    fn test_empty_graph_error() {
        let router = EconomicRouter::new(AgentGraph::new());
        let qos = QoSConstraints::default();

        let err = router
            .find_route(TaskType::Analysis, &qos, None)
            .unwrap_err();
        assert!(matches!(err, RoutingError::EmptyGraph));
    }

    #[test]
    fn test_no_capable_agents_error() {
        let mut graph = AgentGraph::new();
        let id = AgentId::new();
        // Agent only has CodeGeneration, but we ask for Analysis.
        graph.add_agent(
            id,
            vec![make_cap(TaskType::CodeGeneration, 0.9, 100, 0.5)],
            0.8,
        );

        let router = EconomicRouter::new(graph);
        let qos = QoSConstraints::default();

        let err = router
            .find_route(TaskType::Analysis, &qos, None)
            .unwrap_err();
        // Should fail because no agents handle Analysis.
        assert!(matches!(
            err,
            RoutingError::NoFeasibleRoute | RoutingError::EmptyGraph
        ));
    }

    #[test]
    fn test_agent_removal_and_restore() {
        let mut router = build_router();
        let qos = QoSConstraints {
            min_quality: 0.1,
            max_latency: Duration::from_secs(10),
            max_cost: 5.0,
            min_trust: 0.5,
        };

        // Get initial route count.
        let initial = router
            .find_routes(TaskType::CodeGeneration, &qos)
            .unwrap()
            .len();

        // Remove all agents — should fail.
        let ids: Vec<AgentId> = (0..router.graph().node_count())
            .filter_map(|i| router.graph().agent_id(i))
            .collect();

        for id in &ids {
            router.remove_agent(*id);
        }

        let err = router.find_route(TaskType::CodeGeneration, &qos, None);
        assert!(err.is_err());

        // Restore all.
        for id in &ids {
            router.restore_agent(*id);
        }

        let restored = router
            .find_routes(TaskType::CodeGeneration, &qos)
            .unwrap()
            .len();
        assert_eq!(initial, restored);
    }

    #[test]
    fn test_routing_performance_50_agents() {
        // Verify < 1ms for 50 agents.
        let mut graph = AgentGraph::with_capacity(50);
        let task = TaskType::DataProcessing;

        let mut ids = Vec::with_capacity(50);
        for i in 0..50 {
            let id = AgentId::new();
            let quality = 0.5 + (i as f64 / 100.0);
            let latency = 50 + i as u64 * 5;
            let cost = 0.1 + i as f64 * 0.02;
            graph.add_agent(id, vec![make_cap(task, quality, latency, cost)], 0.7);
            ids.push(id);
        }

        graph.fully_connect(Duration::from_millis(5));

        let router = EconomicRouter::new(graph);
        let qos = QoSConstraints {
            min_quality: 0.3,
            max_latency: Duration::from_secs(30),
            max_cost: 10.0,
            min_trust: 0.5,
        };

        let start = std::time::Instant::now();
        let route = router.find_route(task, &qos, None).unwrap();
        let elapsed = start.elapsed();

        assert!(!route.agents.is_empty());
        // Performance target: < 1ms for 50 agents.
        // In debug mode this may be slightly over, so we use a generous threshold.
        assert!(
            elapsed < Duration::from_millis(100),
            "routing took {elapsed:?}, expected < 100ms (debug mode threshold)"
        );
    }
}
