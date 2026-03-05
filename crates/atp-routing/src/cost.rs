//! Cost model for routing decisions.
//!
//! Defines how route costs are computed across three dimensions:
//! - Quality(R) = Product(Q(a_i))        [multiplicative — quality compounds]
//! - Latency(R) = Sum(L(a_i)) + Sum(T(edges))  [additive including transfer]
//! - Cost(R)    = Sum(C(a_i))             [additive]

use std::time::Duration;

use atp_types::{Capability, QoSConstraints, RouteMetrics};

/// Weight vector for multi-objective optimization.
/// Weights are non-negative and should sum to 1.0 for normalization,
/// but this is not enforced to allow flexible weighting.
#[derive(Debug, Clone, Copy)]
pub struct CostWeights {
    /// Weight for quality (higher is better, so we negate in scalar cost).
    pub quality: f64,
    /// Weight for latency (lower is better).
    pub latency: f64,
    /// Weight for cost (lower is better).
    pub cost: f64,
}

impl CostWeights {
    pub fn new(quality: f64, latency: f64, cost: f64) -> Self {
        Self {
            quality,
            latency,
            cost,
        }
    }

    /// Predefined weight vectors that sample the Pareto frontier.
    /// These are used by the modified Bellman-Ford to discover diverse routes.
    pub fn pareto_sample_vectors() -> &'static [CostWeights] {
        static VECTORS: &[CostWeights] = &[
            // Pure objectives
            CostWeights { quality: 1.0, latency: 0.0, cost: 0.0 },
            CostWeights { quality: 0.0, latency: 1.0, cost: 0.0 },
            CostWeights { quality: 0.0, latency: 0.0, cost: 1.0 },
            // Pairwise blends
            CostWeights { quality: 0.5, latency: 0.5, cost: 0.0 },
            CostWeights { quality: 0.5, latency: 0.0, cost: 0.5 },
            CostWeights { quality: 0.0, latency: 0.5, cost: 0.5 },
            // Balanced
            CostWeights { quality: 0.34, latency: 0.33, cost: 0.33 },
            // Quality-heavy
            CostWeights { quality: 0.6, latency: 0.2, cost: 0.2 },
            // Latency-heavy
            CostWeights { quality: 0.2, latency: 0.6, cost: 0.2 },
            // Cost-heavy
            CostWeights { quality: 0.2, latency: 0.2, cost: 0.6 },
        ];
        VECTORS
    }
}

impl Default for CostWeights {
    fn default() -> Self {
        Self {
            quality: 0.34,
            latency: 0.33,
            cost: 0.33,
        }
    }
}

/// The cost model computes scalar and multi-dimensional costs for edges and routes.
#[derive(Debug, Clone)]
pub struct CostModel {
    /// Normalization reference for latency (converts to 0..1 range).
    pub latency_ref: Duration,
    /// Normalization reference for cost (converts to 0..1 range).
    pub cost_ref: f64,
}

impl Default for CostModel {
    fn default() -> Self {
        Self {
            latency_ref: Duration::from_secs(10),
            cost_ref: 1.0,
        }
    }
}

impl CostModel {
    pub fn new(latency_ref: Duration, cost_ref: f64) -> Self {
        Self {
            latency_ref,
            cost_ref,
        }
    }

    /// Compute the scalar edge cost for a single agent capability under given weights.
    ///
    /// Quality is negated because higher quality is better but Bellman-Ford minimizes.
    /// All values are normalized to [0, 1] before weighting.
    pub fn scalar_edge_cost(&self, cap: &Capability, transfer_latency: Duration, weights: &CostWeights) -> f64 {
        let q_norm = 1.0 - cap.estimated_quality.clamp(0.0, 1.0); // negate: lower is better
        let total_latency = cap.estimated_latency + transfer_latency;
        let l_norm = (total_latency.as_secs_f64() / self.latency_ref.as_secs_f64()).min(2.0);
        let c_norm = (cap.cost_per_task / self.cost_ref).min(2.0);

        weights.quality * q_norm + weights.latency * l_norm + weights.cost * c_norm
    }

    /// Compute route metrics from a sequence of capabilities and transfer latencies.
    ///
    /// `transfer_latencies` should have length = `capabilities.len() - 1` (edges between nodes).
    /// If empty route, returns zero metrics.
    pub fn compute_route_metrics(
        &self,
        capabilities: &[Capability],
        transfer_latencies: &[Duration],
    ) -> RouteMetrics {
        if capabilities.is_empty() {
            return RouteMetrics {
                quality: 0.0,
                latency: Duration::ZERO,
                cost: 0.0,
            };
        }

        let quality = capabilities
            .iter()
            .map(|c| c.estimated_quality.clamp(0.0, 1.0))
            .product();

        let agent_latency: Duration = capabilities.iter().map(|c| c.estimated_latency).sum();
        let edge_latency: Duration = transfer_latencies.iter().sum();
        let latency = agent_latency + edge_latency;

        let cost = capabilities.iter().map(|c| c.cost_per_task).sum();

        RouteMetrics {
            quality,
            latency,
            cost,
        }
    }

    /// Scalar cost of a complete route for comparison under given weights.
    pub fn scalar_route_cost(&self, metrics: &RouteMetrics, weights: &CostWeights) -> f64 {
        let q_norm = 1.0 - metrics.quality.clamp(0.0, 1.0);
        let l_norm = (metrics.latency.as_secs_f64() / self.latency_ref.as_secs_f64()).min(2.0);
        let c_norm = (metrics.cost / self.cost_ref).min(2.0);

        weights.quality * q_norm + weights.latency * l_norm + weights.cost * c_norm
    }

    /// Check whether route metrics satisfy QoS constraints.
    pub fn satisfies_constraints(&self, metrics: &RouteMetrics, qos: &QoSConstraints) -> bool {
        metrics.quality >= qos.min_quality
            && metrics.latency <= qos.max_latency
            && metrics.cost <= qos.max_cost
    }

    /// Derive weights from QoS constraints.
    /// Tighter constraints get higher weight.
    pub fn weights_from_constraints(qos: &QoSConstraints) -> CostWeights {
        // Inverse of headroom: tighter constraint -> higher weight
        let q_tightness = qos.min_quality; // 0.9 is tight, 0.1 is loose
        let l_tightness = 1.0 / (1.0 + qos.max_latency.as_secs_f64()); // short deadline = tight
        let c_tightness = 1.0 / (1.0 + qos.max_cost); // low budget = tight

        let total = q_tightness + l_tightness + c_tightness;
        if total < f64::EPSILON {
            return CostWeights::default();
        }

        CostWeights {
            quality: q_tightness / total,
            latency: l_tightness / total,
            cost: c_tightness / total,
        }
    }

    /// Estimate transfer latency between two agents.
    /// In a real system this would come from network measurements.
    /// Default: 5ms per hop.
    pub fn default_transfer_latency() -> Duration {
        Duration::from_millis(5)
    }

    /// Sort capabilities by cost (cheapest first) for a given task type.
    pub fn sort_by_cost(caps: &mut [(usize, &Capability)]) {
        caps.sort_by(|a, b| {
            a.1.cost_per_task
                .partial_cmp(&b.1.cost_per_task)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    }

    /// Sort capabilities by quality (highest first) for a given task type.
    pub fn sort_by_quality(caps: &mut [(usize, &Capability)]) {
        caps.sort_by(|a, b| {
            b.1.estimated_quality
                .partial_cmp(&a.1.estimated_quality)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    }
}

/// Estimate the savings fraction for a routing pattern.
pub fn estimated_savings(pattern: atp_types::RoutingPattern) -> (f64, f64) {
    use atp_types::RoutingPattern;
    match pattern {
        RoutingPattern::DraftRefine => (0.40, 0.70),
        RoutingPattern::Cascade => (0.30, 0.50),
        RoutingPattern::ParallelMerge => (0.0, 0.10), // latency savings, not cost
        RoutingPattern::Ensemble => (0.0, 0.0),        // quality focus, no cost savings
        RoutingPattern::Pipeline => (0.0, 0.20),       // modest savings through specialization
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use atp_types::TaskType;

    fn sample_capability(quality: f64, latency_ms: u64, cost: f64) -> Capability {
        Capability {
            task_type: TaskType::CodeGeneration,
            estimated_quality: quality,
            estimated_latency: Duration::from_millis(latency_ms),
            cost_per_task: cost,
        }
    }

    #[test]
    fn test_route_metrics_multiplicative_quality() {
        let model = CostModel::default();
        let caps = vec![
            sample_capability(0.9, 100, 0.5),
            sample_capability(0.8, 200, 0.3),
        ];
        let transfers = vec![Duration::from_millis(5)];
        let metrics = model.compute_route_metrics(&caps, &transfers);

        assert!((metrics.quality - 0.72).abs() < 1e-6);
        assert_eq!(metrics.latency, Duration::from_millis(305));
        assert!((metrics.cost - 0.8).abs() < 1e-6);
    }

    #[test]
    fn test_empty_route_metrics() {
        let model = CostModel::default();
        let metrics = model.compute_route_metrics(&[], &[]);
        assert!((metrics.quality - 0.0).abs() < 1e-6);
        assert_eq!(metrics.latency, Duration::ZERO);
        assert!((metrics.cost - 0.0).abs() < 1e-6);
    }

    #[test]
    fn test_scalar_edge_cost_pure_quality() {
        let model = CostModel::default();
        let weights = CostWeights::new(1.0, 0.0, 0.0);
        let cap = sample_capability(0.9, 100, 0.5);
        let cost = model.scalar_edge_cost(&cap, Duration::ZERO, &weights);
        // 1.0 * (1.0 - 0.9) = 0.1
        assert!((cost - 0.1).abs() < 1e-6);
    }

    #[test]
    fn test_satisfies_constraints() {
        let model = CostModel::default();
        let metrics = RouteMetrics {
            quality: 0.8,
            latency: Duration::from_secs(5),
            cost: 0.5,
        };
        let qos = QoSConstraints::default();
        assert!(model.satisfies_constraints(&metrics, &qos));

        let tight_qos = QoSConstraints {
            min_quality: 0.9,
            ..Default::default()
        };
        assert!(!model.satisfies_constraints(&metrics, &tight_qos));
    }
}
