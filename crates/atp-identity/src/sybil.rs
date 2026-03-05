//! Sybil resistance via transitive trust dampening.
//!
//! # Formula
//!
//! ```text
//! T_transitive(a, via b) = T_direct(a) + α × T(b) × T_attestation(b → a)
//! ```
//!
//! where `α = 0.5` is the transitive dampening factor. This ensures that
//! trust accumulated through intermediary vouches is strictly bounded
//! and diminishes with each hop.

use chrono::{DateTime, Utc};

use atp_types::{AgentId, InteractionRecord};

use crate::trust::TrustEngine;

/// Default transitive dampening factor α.
pub const DEFAULT_ALPHA: f64 = 0.5;

/// Maximum attestation chain depth to prevent unbounded recursion.
pub const MAX_CHAIN_DEPTH: usize = 5;

/// Sybil-resistance guard implementing transitive trust dampening.
pub struct SybilGuard {
    /// Transitive dampening factor α ∈ (0, 1).
    alpha: f64,
    /// Maximum chain depth for multi-hop transitive trust.
    max_depth: usize,
    /// Underlying trust engine for direct trust computation.
    trust_engine: TrustEngine,
}

impl Default for SybilGuard {
    fn default() -> Self {
        Self {
            alpha: DEFAULT_ALPHA,
            max_depth: MAX_CHAIN_DEPTH,
            trust_engine: TrustEngine::default(),
        }
    }
}

impl SybilGuard {
    /// Create a guard with custom dampening factor and chain depth.
    pub fn new(alpha: f64, max_depth: usize, trust_engine: TrustEngine) -> Self {
        Self {
            alpha: alpha.clamp(0.0, 1.0),
            max_depth,
            trust_engine,
        }
    }

    /// Return a reference to the underlying trust engine.
    pub fn trust_engine(&self) -> &TrustEngine {
        &self.trust_engine
    }

    /// Compute **direct** aggregate trust for `subject`.
    pub fn direct_trust(
        &self,
        subject: AgentId,
        records: &[InteractionRecord],
        now: DateTime<Utc>,
    ) -> f64 {
        self.trust_engine
            .compute_aggregate_trust(subject, records, now)
    }

    /// Compute **single-hop transitive** trust for `subject` via `voucher`.
    ///
    /// ```text
    /// T_transitive = T_direct(subject) + α × T(voucher) × T_attestation(voucher → subject)
    /// ```
    ///
    /// - `records` contains all interaction records in the system.
    /// - `attestation_records` contains records *from voucher about subject*.
    pub fn transitive_trust(
        &self,
        subject: AgentId,
        voucher: AgentId,
        records: &[InteractionRecord],
        now: DateTime<Utc>,
    ) -> f64 {
        let t_direct = self.direct_trust(subject, records, now);
        let t_voucher = self.direct_trust(voucher, records, now);

        // Attestation: the voucher's direct assessment of the subject.
        let voucher_about_subject: Vec<InteractionRecord> = records
            .iter()
            .filter(|r| r.evaluator == voucher && r.subject == subject)
            .cloned()
            .collect();

        let t_attestation = self
            .trust_engine
            .compute_aggregate_trust(subject, &voucher_about_subject, now);

        let transitive_component = self.alpha * t_voucher * t_attestation;

        (t_direct + transitive_component).clamp(0.0, 1.0)
    }

    /// Compute **multi-hop transitive** trust for `subject` along a
    /// chain of vouchers `[v_1, v_2, ..., v_n]`.
    ///
    /// Each hop applies the dampening factor α, so the contribution
    /// of hop k is `α^k`. The chain is truncated at [`MAX_CHAIN_DEPTH`].
    pub fn chain_trust(
        &self,
        subject: AgentId,
        chain: &[AgentId],
        records: &[InteractionRecord],
        now: DateTime<Utc>,
    ) -> f64 {
        let t_direct = self.direct_trust(subject, records, now);

        if chain.is_empty() {
            return t_direct;
        }

        let depth = chain.len().min(self.max_depth);
        let mut transitive_sum = 0.0_f64;

        for (i, &voucher) in chain.iter().take(depth).enumerate() {
            let alpha_k = self.alpha.powi((i + 1) as i32);
            let t_voucher = self.direct_trust(voucher, records, now);

            // Attestation from this voucher about the subject
            let voucher_about_subject: Vec<InteractionRecord> = records
                .iter()
                .filter(|r| r.evaluator == voucher && r.subject == subject)
                .cloned()
                .collect();

            let t_attestation = self
                .trust_engine
                .compute_aggregate_trust(subject, &voucher_about_subject, now);

            transitive_sum += alpha_k * t_voucher * t_attestation;
        }

        (t_direct + transitive_sum).clamp(0.0, 1.0)
    }

    /// Check whether `subject` meets a minimum trust threshold, using
    /// transitive trust through an optional voucher chain.
    pub fn meets_threshold(
        &self,
        subject: AgentId,
        chain: &[AgentId],
        records: &[InteractionRecord],
        threshold: f64,
        now: DateTime<Utc>,
    ) -> bool {
        let trust = if chain.is_empty() {
            self.direct_trust(subject, records, now)
        } else {
            self.chain_trust(subject, chain, records, now)
        };
        trust >= threshold
    }

    /// Detect potential Sybil behaviour: if an agent has *many* vouchers
    /// but very few direct interaction records, that is suspicious.
    ///
    /// Returns a suspicion score in `[0, 1]` — higher means more suspicious.
    pub fn sybil_suspicion(
        &self,
        subject: AgentId,
        records: &[InteractionRecord],
    ) -> f64 {
        let direct_count = records
            .iter()
            .filter(|r| r.subject == subject)
            .count();

        let voucher_count = records
            .iter()
            .filter(|r| r.subject == subject)
            .map(|r| r.evaluator)
            .collect::<std::collections::HashSet<_>>()
            .len();

        if voucher_count == 0 {
            return 0.0;
        }

        // Ratio: many unique evaluators with few total records is suspicious
        // (indicates potential Sybil sock-puppet attestations).
        if direct_count == 0 {
            return 1.0;
        }

        let ratio = voucher_count as f64 / direct_count as f64;
        // A ratio near 1.0 means each evaluator gave exactly one record —
        // suspicious if total count is low.
        if direct_count < 3 && ratio > 0.8 {
            return ratio.clamp(0.0, 1.0);
        }

        // Otherwise, suspicion decays with more records
        (ratio / (1.0 + direct_count as f64 * 0.1)).clamp(0.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use atp_types::TaskType;
    use chrono::Duration;

    fn make_record(
        evaluator: AgentId,
        subject: AgentId,
        quality: f64,
        days_ago: i64,
        now: DateTime<Utc>,
    ) -> InteractionRecord {
        InteractionRecord {
            evaluator,
            subject,
            task_type: TaskType::Analysis,
            quality_score: quality,
            latency_ms: 50,
            cost: 0.01,
            timestamp: now - Duration::days(days_ago),
            signature: Vec::new(),
        }
    }

    #[test]
    fn transitive_trust_adds_voucher_contribution() {
        let guard = SybilGuard::default();
        let alice = AgentId::new();
        let bob = AgentId::new(); // voucher
        let subject = AgentId::new();
        let now = Utc::now();

        let records = vec![
            // Bob is well-trusted (high quality from alice)
            make_record(alice, bob, 0.9, 0, now),
            // Bob vouches for subject
            make_record(bob, subject, 0.8, 0, now),
        ];

        let direct = guard.direct_trust(subject, &records, now);
        let transitive = guard.transitive_trust(subject, bob, &records, now);

        // Transitive should be >= direct
        assert!(transitive >= direct);
    }

    #[test]
    fn dampening_reduces_with_chain_length() {
        let guard = SybilGuard::default();
        let a = AgentId::new();
        let v1 = AgentId::new();
        let v2 = AgentId::new();
        let subject = AgentId::new();
        let now = Utc::now();

        let records = vec![
            make_record(a, v1, 0.9, 0, now),
            make_record(a, v2, 0.9, 0, now),
            make_record(v1, subject, 0.8, 0, now),
            make_record(v2, subject, 0.8, 0, now),
        ];

        let t1 = guard.chain_trust(subject, &[v1], &records, now);
        let t2 = guard.chain_trust(subject, &[v1, v2], &records, now);

        // Longer chain adds more but each hop is dampened
        assert!(t2 >= t1);
        // But the second hop adds less than the first hop added
        let direct = guard.direct_trust(subject, &records, now);
        let hop1_contribution = t1 - direct;
        let hop2_contribution = t2 - t1;
        assert!(hop2_contribution <= hop1_contribution);
    }

    #[test]
    fn no_voucher_returns_direct() {
        let guard = SybilGuard::default();
        let subject = AgentId::new();
        let now = Utc::now();

        let direct = guard.direct_trust(subject, &[], now);
        let chain = guard.chain_trust(subject, &[], &[], now);
        assert!((direct - chain).abs() < 1e-12);
    }

    #[test]
    fn meets_threshold_works() {
        let guard = SybilGuard::default();
        let subject = AgentId::new();
        let now = Utc::now();

        // Default prior = 0.5
        assert!(guard.meets_threshold(subject, &[], &[], 0.5, now));
        assert!(!guard.meets_threshold(subject, &[], &[], 0.6, now));
    }

    #[test]
    fn sybil_suspicion_for_empty() {
        let guard = SybilGuard::default();
        let subject = AgentId::new();
        let score = guard.sybil_suspicion(subject, &[]);
        assert!((score - 0.0).abs() < 1e-12);
    }
}
