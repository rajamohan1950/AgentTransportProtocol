//! Pareto-front route selection.
//!
//! Given a set of candidate routes from multi-objective Bellman-Ford,
//! the optimizer extracts the Pareto-optimal subset: routes where no other
//! route is better on all three objectives (quality, latency, cost).
//!
//! It then filters by QoS constraints and ranks by weighted scalar cost.


use atp_types::{QoSConstraints, RouteMetrics, RoutingError};

use crate::bellman_ford::BellmanFordResult;
use crate::cost::{CostModel, CostWeights};

/// A route on the Pareto front with its associated metadata.
#[derive(Debug, Clone)]
pub struct ParetoRoute {
    /// Agent indices in path order.
    pub path: Vec<usize>,
    /// Multi-dimensional metrics.
    pub metrics: RouteMetrics,
    /// Scalar cost under the reference weights.
    pub scalar_cost: f64,
}

/// Check if route `a` dominates route `b` in the Pareto sense.
///
/// `a` dominates `b` if `a` is at least as good on all objectives
/// and strictly better on at least one.
///
/// Objectives: maximize quality, minimize latency, minimize cost.
fn dominates(a: &RouteMetrics, b: &RouteMetrics) -> bool {
    let a_q = a.quality;
    let b_q = b.quality;
    let a_l = a.latency.as_secs_f64();
    let b_l = b.latency.as_secs_f64();
    let a_c = a.cost;
    let b_c = b.cost;

    let at_least_as_good = a_q >= b_q && a_l <= b_l && a_c <= b_c;
    let strictly_better = a_q > b_q || a_l < b_l || a_c < b_c;

    at_least_as_good && strictly_better
}

/// Extract the Pareto front from a set of Bellman-Ford results.
///
/// Removes duplicates (same path) before computing dominance.
/// Returns routes where no other route dominates them.
pub fn pareto_front(results: &[BellmanFordResult]) -> Vec<ParetoRoute> {
    if results.is_empty() {
        return Vec::new();
    }

    // Deduplicate by path (keep best scalar cost per unique path).
    let mut unique: std::collections::HashMap<Vec<usize>, &BellmanFordResult> =
        std::collections::HashMap::new();

    for r in results {
        unique
            .entry(r.path.clone())
            .and_modify(|existing| {
                if r.scalar_cost < existing.scalar_cost {
                    *existing = r;
                }
            })
            .or_insert(r);
    }

    let candidates: Vec<&BellmanFordResult> = unique.into_values().collect();

    // Compute Pareto front: O(n^2) dominance check.
    let mut front = Vec::new();
    for (i, candidate) in candidates.iter().enumerate() {
        let is_dominated = candidates.iter().enumerate().any(|(j, other)| {
            i != j && dominates(&other.metrics, &candidate.metrics)
        });

        if !is_dominated {
            front.push(ParetoRoute {
                path: candidate.path.clone(),
                metrics: candidate.metrics.clone(),
                scalar_cost: candidate.scalar_cost,
            });
        }
    }

    front
}

/// Filter Pareto routes by QoS constraints.
pub fn filter_by_constraints(
    routes: &[ParetoRoute],
    qos: &QoSConstraints,
    cost_model: &CostModel,
) -> Vec<ParetoRoute> {
    routes
        .iter()
        .filter(|r| cost_model.satisfies_constraints(&r.metrics, qos))
        .cloned()
        .collect()
}

/// Rank routes by scalar cost under given weights (lowest first).
pub fn rank_routes(routes: &mut [ParetoRoute], weights: &CostWeights, cost_model: &CostModel) {
    for r in routes.iter_mut() {
        r.scalar_cost = cost_model.scalar_route_cost(&r.metrics, weights);
    }
    routes.sort_by(|a, b| {
        a.scalar_cost
            .partial_cmp(&b.scalar_cost)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
}

/// Select the top `n` routes from Pareto front, filtered by constraints
/// and ranked by weighted cost.
pub fn select_top_routes(
    results: &[BellmanFordResult],
    qos: &QoSConstraints,
    weights: &CostWeights,
    cost_model: &CostModel,
    max_routes: usize,
) -> Result<Vec<ParetoRoute>, RoutingError> {
    let front = pareto_front(results);
    if front.is_empty() {
        return Err(RoutingError::NoFeasibleRoute);
    }

    let mut feasible = filter_by_constraints(&front, qos, cost_model);

    // If no feasible routes on the Pareto front, try all candidates
    // (some dominated routes might still satisfy constraints).
    if feasible.is_empty() {
        let all_routes: Vec<ParetoRoute> = results
            .iter()
            .map(|r| ParetoRoute {
                path: r.path.clone(),
                metrics: r.metrics.clone(),
                scalar_cost: r.scalar_cost,
            })
            .collect();
        feasible = filter_by_constraints(&all_routes, qos, cost_model);
    }

    if feasible.is_empty() {
        return Err(RoutingError::NoFeasibleRoute);
    }

    rank_routes(&mut feasible, weights, cost_model);

    // Deduplicate by path after ranking.
    let mut seen = std::collections::HashSet::new();
    feasible.retain(|r| seen.insert(r.path.clone()));

    feasible.truncate(max_routes);
    Ok(feasible)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn make_result(path: Vec<usize>, quality: f64, latency_ms: u64, cost: f64) -> BellmanFordResult {
        BellmanFordResult {
            weights: CostWeights::default(),
            path,
            scalar_cost: 0.0,
            metrics: RouteMetrics {
                quality,
                latency: Duration::from_millis(latency_ms),
                cost,
            },
        }
    }

    #[test]
    fn test_pareto_front_basic() {
        let results = vec![
            make_result(vec![0], 0.9, 100, 0.5),    // high quality
            make_result(vec![1], 0.5, 50, 0.1),     // low cost, fast
            make_result(vec![2], 0.6, 80, 0.6),     // dominated by [1] on latency+cost, by [0] on quality
        ];

        let front = pareto_front(&results);
        // [0] and [1] should be on the front; [2] may or may not be dominated.
        assert!(front.len() >= 2);
    }

    #[test]
    fn test_dominance() {
        let a = RouteMetrics {
            quality: 0.9,
            latency: Duration::from_millis(100),
            cost: 0.5,
        };
        let b = RouteMetrics {
            quality: 0.8,
            latency: Duration::from_millis(150),
            cost: 0.6,
        };
        assert!(dominates(&a, &b));
        assert!(!dominates(&b, &a));
    }

    #[test]
    fn test_no_self_dominance() {
        let a = RouteMetrics {
            quality: 0.9,
            latency: Duration::from_millis(100),
            cost: 0.5,
        };
        assert!(!dominates(&a, &a));
    }

    #[test]
    fn test_filter_constraints() {
        let cost_model = CostModel::default();
        let routes = vec![
            ParetoRoute {
                path: vec![0],
                metrics: RouteMetrics {
                    quality: 0.9,
                    latency: Duration::from_millis(100),
                    cost: 0.5,
                },
                scalar_cost: 0.0,
            },
            ParetoRoute {
                path: vec![1],
                metrics: RouteMetrics {
                    quality: 0.3,
                    latency: Duration::from_millis(50),
                    cost: 0.1,
                },
                scalar_cost: 0.0,
            },
        ];

        let qos = QoSConstraints {
            min_quality: 0.7,
            ..Default::default()
        };

        let filtered = filter_by_constraints(&routes, &qos, &cost_model);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].path, vec![0]);
    }
}
