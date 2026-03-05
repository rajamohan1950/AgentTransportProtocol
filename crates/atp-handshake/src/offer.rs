//! Phase 2: CAPABILITY_OFFER creation and ranking.
//!
//! When an agent's capabilities match a probe, it produces a
//! `CAPABILITY_OFFER` — a bid containing its quality, latency, cost, and
//! trust score.  The requester collects offers during the phase-2 timeout
//! window and ranks them using a weighted composite score.

use atp_types::{AgentId, Capability, CapabilityOfferMsg, CapabilityProbeMsg};
use chrono::Utc;
use std::time::Duration;

/// Create a CAPABILITY_OFFER in response to a probe.
///
/// The offer references the probe via `in_reply_to` (the probe nonce),
/// includes the agent's capability metrics and trust score, and sets a
/// TTL after which the offer expires.
///
/// The `signature` and `trust_proof` fields are left empty and should be
/// filled by the caller's signing layer.
pub fn create_offer(
    from: AgentId,
    probe: &CapabilityProbeMsg,
    capability: Capability,
    trust_score: f64,
    ttl: Duration,
) -> CapabilityOfferMsg {
    CapabilityOfferMsg {
        from,
        in_reply_to: probe.nonce,
        capability,
        trust_score,
        trust_proof: Vec::new(),
        ttl,
        timestamp: Utc::now(),
        signature: Vec::new(),
    }
}

/// Default offer TTL (5 seconds).
pub const DEFAULT_OFFER_TTL: Duration = Duration::from_secs(5);

/// Weights used to compute the composite offer score.
///
/// The four dimensions (quality, latency, cost, trust) are normalised to
/// `[0, 1]` and combined as a weighted sum.  Default weights emphasise
/// quality and trust over latency and cost.
#[derive(Debug, Clone)]
pub struct OfferWeights {
    /// Weight for estimated quality (higher is better). Default: 0.35.
    pub quality: f64,
    /// Weight for latency (lower is better, inverted in scoring). Default: 0.20.
    pub latency: f64,
    /// Weight for cost (lower is better, inverted in scoring). Default: 0.15.
    pub cost: f64,
    /// Weight for trust score (higher is better). Default: 0.30.
    pub trust: f64,
}

impl Default for OfferWeights {
    fn default() -> Self {
        Self {
            quality: 0.35,
            latency: 0.20,
            cost: 0.15,
            trust: 0.30,
        }
    }
}

/// An offer together with its computed composite score.
#[derive(Debug, Clone)]
pub struct RankedOffer {
    pub offer: CapabilityOfferMsg,
    pub score: f64,
}

/// Stateless offer ranking engine.
#[derive(Debug, Clone)]
pub struct OfferRanker {
    weights: OfferWeights,
    /// Reference maximum latency (for normalisation).  Any latency at or
    /// above this value scores 0 on the latency dimension.
    max_latency: Duration,
    /// Reference maximum cost (for normalisation).
    max_cost: f64,
}

impl OfferRanker {
    /// Create a ranker with custom weights and normalisation bounds.
    pub fn new(weights: OfferWeights, max_latency: Duration, max_cost: f64) -> Self {
        Self {
            weights,
            max_latency,
            max_cost,
        }
    }

    /// Create a ranker with default weights, deriving normalisation bounds
    /// from QoS constraints.
    pub fn from_defaults(max_latency: Duration, max_cost: f64) -> Self {
        Self {
            weights: OfferWeights::default(),
            max_latency,
            max_cost,
        }
    }

    /// Score a single offer.  Returns a value in `[0.0, 1.0]` where
    /// higher is better.
    pub fn score(&self, offer: &CapabilityOfferMsg) -> f64 {
        let w = &self.weights;

        // Quality: already in [0, 1].
        let q = offer.capability.estimated_quality.clamp(0.0, 1.0);

        // Latency: invert so lower latency → higher score.
        let max_lat_secs = self.max_latency.as_secs_f64();
        let lat_score = if max_lat_secs > 0.0 {
            (1.0 - offer.capability.estimated_latency.as_secs_f64() / max_lat_secs).clamp(0.0, 1.0)
        } else {
            1.0
        };

        // Cost: invert so lower cost → higher score.
        let cost_score = if self.max_cost > 0.0 {
            (1.0 - offer.capability.cost_per_task / self.max_cost).clamp(0.0, 1.0)
        } else {
            1.0
        };

        // Trust: already in [0, 1].
        let t = offer.trust_score.clamp(0.0, 1.0);

        w.quality * q + w.latency * lat_score + w.cost * cost_score + w.trust * t
    }

    /// Rank a slice of offers by composite score (best first).
    pub fn rank(&self, offers: &[CapabilityOfferMsg]) -> Vec<RankedOffer> {
        let mut ranked: Vec<RankedOffer> = offers
            .iter()
            .map(|o| RankedOffer {
                score: self.score(o),
                offer: o.clone(),
            })
            .collect();

        ranked.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        ranked
    }
}

/// Convenience function: rank a list of offers with default weights,
/// using the provided QoS max-latency and max-cost as normalisation
/// bounds.
pub fn rank_offers(
    offers: &[CapabilityOfferMsg],
    max_latency: Duration,
    max_cost: f64,
) -> Vec<RankedOffer> {
    let ranker = OfferRanker::from_defaults(max_latency, max_cost);
    ranker.rank(offers)
}

/// Check whether an offer has expired based on its timestamp and TTL.
pub fn is_offer_expired(offer: &CapabilityOfferMsg) -> bool {
    let now = Utc::now();
    let expires_at = offer.timestamp + chrono::Duration::from_std(offer.ttl).unwrap_or_default();
    now > expires_at
}

#[cfg(test)]
mod tests {
    use super::*;
    use atp_types::{Capability, QoSConstraints, TaskType};

    fn make_offer(
        quality: f64,
        latency_ms: u64,
        cost: f64,
        trust: f64,
        nonce: u64,
    ) -> CapabilityOfferMsg {
        CapabilityOfferMsg {
            from: AgentId::new(),
            in_reply_to: nonce,
            capability: Capability {
                task_type: TaskType::CodeGeneration,
                estimated_quality: quality,
                estimated_latency: Duration::from_millis(latency_ms),
                cost_per_task: cost,
            },
            trust_score: trust,
            trust_proof: Vec::new(),
            ttl: Duration::from_secs(5),
            timestamp: Utc::now(),
            signature: Vec::new(),
        }
    }

    #[test]
    fn create_offer_sets_in_reply_to() {
        let probe = crate::probe::create_probe(
            AgentId::new(),
            TaskType::Analysis,
            QoSConstraints::default(),
            None,
        );
        let cap = Capability {
            task_type: TaskType::Analysis,
            estimated_quality: 0.9,
            estimated_latency: Duration::from_millis(100),
            cost_per_task: 0.5,
        };
        let offer = create_offer(AgentId::new(), &probe, cap, 0.8, DEFAULT_OFFER_TTL);
        assert_eq!(offer.in_reply_to, probe.nonce);
    }

    #[test]
    fn ranker_prefers_higher_quality() {
        let o1 = make_offer(0.9, 100, 0.5, 0.8, 42);
        let o2 = make_offer(0.6, 100, 0.5, 0.8, 42);

        let ranked = rank_offers(&[o2, o1], Duration::from_secs(1), 1.0);
        assert!(ranked[0].score > ranked[1].score);
        assert!((ranked[0].offer.capability.estimated_quality - 0.9).abs() < f64::EPSILON);
    }

    #[test]
    fn ranker_prefers_lower_latency() {
        let o1 = make_offer(0.8, 50, 0.5, 0.8, 42);
        let o2 = make_offer(0.8, 500, 0.5, 0.8, 42);

        let ranked = rank_offers(&[o2, o1], Duration::from_secs(1), 1.0);
        assert!(ranked[0].score > ranked[1].score);
    }

    #[test]
    fn ranker_prefers_lower_cost() {
        let o1 = make_offer(0.8, 100, 0.1, 0.8, 42);
        let o2 = make_offer(0.8, 100, 0.9, 0.8, 42);

        let ranked = rank_offers(&[o2, o1], Duration::from_secs(1), 1.0);
        assert!(ranked[0].score > ranked[1].score);
    }

    #[test]
    fn ranker_prefers_higher_trust() {
        let o1 = make_offer(0.8, 100, 0.5, 0.95, 42);
        let o2 = make_offer(0.8, 100, 0.5, 0.3, 42);

        let ranked = rank_offers(&[o2, o1], Duration::from_secs(1), 1.0);
        assert!(ranked[0].score > ranked[1].score);
    }

    #[test]
    fn score_in_unit_range() {
        let ranker = OfferRanker::from_defaults(Duration::from_secs(1), 1.0);
        // Perfect offer
        let best = make_offer(1.0, 0, 0.0, 1.0, 1);
        let s = ranker.score(&best);
        assert!((0.0..=1.0).contains(&s), "score was {s}");
        assert!((s - 1.0).abs() < f64::EPSILON);

        // Worst offer
        let worst = make_offer(0.0, 1000, 1.0, 0.0, 1);
        let s = ranker.score(&worst);
        assert!((0.0..=1.0).contains(&s), "score was {s}");
        assert!(s.abs() < f64::EPSILON);
    }
}
