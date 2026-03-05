use crate::AgentId;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// A routing pattern for task execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RoutingPattern {
    /// Cheap agent drafts, expensive agent refines. 40-70% cost saving.
    DraftRefine,
    /// Multiple agents process independently, results merged.
    ParallelMerge,
    /// Try cheapest first, escalate on low confidence. 30-50% cost saving.
    Cascade,
    /// Multiple agents vote on result. Quality focus.
    Ensemble,
    /// Sequential processing chain.
    Pipeline,
}

impl std::fmt::Display for RoutingPattern {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RoutingPattern::DraftRefine => write!(f, "draft_refine"),
            RoutingPattern::ParallelMerge => write!(f, "parallel_merge"),
            RoutingPattern::Cascade => write!(f, "cascade"),
            RoutingPattern::Ensemble => write!(f, "ensemble"),
            RoutingPattern::Pipeline => write!(f, "pipeline"),
        }
    }
}

/// Metrics for a computed route.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteMetrics {
    /// Multiplicative: Product(Q(a_i))
    pub quality: f64,
    /// Additive: Sum(L(a_i)) + Sum(T(edges))
    pub latency: Duration,
    /// Additive: Sum(C(a_i))
    pub cost: f64,
}

/// A computed route through the agent network.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Route {
    pub id: uuid::Uuid,
    pub pattern: RoutingPattern,
    pub agents: Vec<AgentId>,
    pub metrics: RouteMetrics,
    pub computed_at: chrono::DateTime<chrono::Utc>,
    pub ttl: Duration,
}
