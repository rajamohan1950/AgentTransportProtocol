//! Time-decayed trust score computation.
//!
//! # Formula
//!
//! ```text
//! T(a) = Sigma(q_i * w(tau_i) * gamma(task_type_i)) / Sigma(w(tau_i) * gamma(task_type_i))
//! ```
//!
//! where
//!
//! - `q_i` is the quality score of interaction `i`,
//! - `w(tau_i) = e^(-lambda * (t_now - t_i))` is an exponential time-decay weight,
//! - `lambda = 0.01` per day,
//! - `gamma(task_type)` is `TaskType::complexity_weight`.

use chrono::{DateTime, Utc};

use atp_types::{AgentId, InteractionRecord, TaskType, TrustScore, TrustVector};

/// Default time-decay constant lambda (per day).
pub const DEFAULT_LAMBDA: f64 = 0.01;

/// Default prior trust score for agents with no interaction history.
pub const DEFAULT_PRIOR: f64 = 0.5;

/// Engine for computing trust scores from interaction records.
pub struct TrustEngine {
    /// Decay constant lambda (units: per day).
    lambda: f64,
    /// Prior trust score used when no records exist.
    prior: f64,
}

impl Default for TrustEngine {
    fn default() -> Self {
        Self {
            lambda: DEFAULT_LAMBDA,
            prior: DEFAULT_PRIOR,
        }
    }
}

impl TrustEngine {
    /// Create a new engine with custom parameters.
    pub fn new(lambda: f64, prior: f64) -> Self {
        Self { lambda, prior }
    }

    // -- Single task-type trust ---

    /// Compute the trust score for `subject` restricted to `task_type`,
    /// as evaluated by the records in `records` (which should already be
    /// filtered to the evaluator perspective if desired).
    ///
    /// Returns a `TrustScore` struct. If no qualifying records exist
    /// the score defaults to `DEFAULT_PRIOR`.
    pub fn compute_trust_score(
        &self,
        subject: AgentId,
        task_type: TaskType,
        records: &[InteractionRecord],
        now: DateTime<Utc>,
    ) -> TrustScore {
        let relevant: Vec<&InteractionRecord> = records
            .iter()
            .filter(|r| r.subject == subject && r.task_type == task_type)
            .collect();

        let (score, count) = self.weighted_average(&relevant, now);

        TrustScore {
            agent: subject,
            task_type,
            score,
            sample_count: count,
            last_updated: now,
        }
    }

    // -- Full trust vector ---

    /// Compute a full `TrustVector` for `subject` across all task types
    /// present in the supplied records.
    pub fn compute_trust_vector(
        &self,
        subject: AgentId,
        records: &[InteractionRecord],
        now: DateTime<Utc>,
    ) -> TrustVector {
        let mut tv = TrustVector::default();

        for &tt in TaskType::all() {
            let ts = self.compute_trust_score(subject, tt, records, now);
            tv.set(tt, ts.score);
        }

        tv
    }

    // -- Aggregate trust (all task types) ---

    /// Compute a single aggregate trust value for `subject` across *all*
    /// task types, applying complexity weights gamma.
    pub fn compute_aggregate_trust(
        &self,
        subject: AgentId,
        records: &[InteractionRecord],
        now: DateTime<Utc>,
    ) -> f64 {
        let relevant: Vec<&InteractionRecord> = records
            .iter()
            .filter(|r| r.subject == subject)
            .collect();

        if relevant.is_empty() {
            return self.prior;
        }

        let mut numerator = 0.0_f64;
        let mut denominator = 0.0_f64;

        for record in &relevant {
            let days_ago = self.days_since(record.timestamp, now);
            let w = (-self.lambda * days_ago).exp();
            let gamma = record.task_type.complexity_weight();
            let weight = w * gamma;

            numerator += record.quality_score * weight;
            denominator += weight;
        }

        if denominator == 0.0 {
            self.prior
        } else {
            (numerator / denominator).clamp(0.0, 1.0)
        }
    }

    // -- Time-decay helper ---

    /// Compute the exponential decay weight `e^(-lambda * days_ago)`.
    pub fn decay_weight(&self, days_ago: f64) -> f64 {
        (-self.lambda * days_ago).exp()
    }

    // -- Internal ---

    /// Weighted average over a set of interaction records:
    ///
    /// ```text
    /// Sigma(q_i * w_i * gamma_i) / Sigma(w_i * gamma_i)
    /// ```
    ///
    /// Returns `(score, sample_count)`.
    fn weighted_average(
        &self,
        records: &[&InteractionRecord],
        now: DateTime<Utc>,
    ) -> (f64, u32) {
        if records.is_empty() {
            return (self.prior, 0);
        }

        let mut numerator = 0.0_f64;
        let mut denominator = 0.0_f64;

        for record in records {
            let days_ago = self.days_since(record.timestamp, now);
            let w = (-self.lambda * days_ago).exp();
            let gamma = record.task_type.complexity_weight();
            let weight = w * gamma;

            numerator += record.quality_score * weight;
            denominator += weight;
        }

        if denominator == 0.0 {
            (self.prior, 0)
        } else {
            let score = (numerator / denominator).clamp(0.0, 1.0);
            (score, records.len() as u32)
        }
    }

    /// Number of days between `timestamp` and `now` (non-negative).
    fn days_since(&self, timestamp: DateTime<Utc>, now: DateTime<Utc>) -> f64 {
        let duration = now.signed_duration_since(timestamp);
        (duration.num_seconds() as f64 / 86_400.0).max(0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn make_record(
        evaluator: AgentId,
        subject: AgentId,
        task_type: TaskType,
        quality: f64,
        days_ago: i64,
        now: DateTime<Utc>,
    ) -> InteractionRecord {
        InteractionRecord {
            evaluator,
            subject,
            task_type,
            quality_score: quality,
            latency_ms: 100,
            cost: 0.01,
            timestamp: now - Duration::days(days_ago),
            signature: Vec::new(),
        }
    }

    #[test]
    fn no_records_returns_prior() {
        let engine = TrustEngine::default();
        let subject = AgentId::new();
        let score = engine.compute_trust_score(
            subject,
            TaskType::CodeGeneration,
            &[],
            Utc::now(),
        );
        assert!((score.score - DEFAULT_PRIOR).abs() < 1e-9);
        assert_eq!(score.sample_count, 0);
    }

    #[test]
    fn single_perfect_record() {
        let engine = TrustEngine::default();
        let evaluator = AgentId::new();
        let subject = AgentId::new();
        let now = Utc::now();

        let records = vec![make_record(
            evaluator,
            subject,
            TaskType::Analysis,
            1.0,
            0,
            now,
        )];

        let score = engine.compute_trust_score(subject, TaskType::Analysis, &records, now);
        assert!((score.score - 1.0).abs() < 1e-6);
        assert_eq!(score.sample_count, 1);
    }

    #[test]
    fn decay_reduces_old_records() {
        let engine = TrustEngine::default();
        let evaluator = AgentId::new();
        let subject = AgentId::new();
        let now = Utc::now();

        // One recent high-quality record (today) and one old low-quality
        // record (100 days ago). The recent record should dominate.
        let records = vec![
            make_record(evaluator, subject, TaskType::CodeGeneration, 0.9, 0, now),
            make_record(evaluator, subject, TaskType::CodeGeneration, 0.1, 100, now),
        ];

        let score = engine.compute_trust_score(
            subject,
            TaskType::CodeGeneration,
            &records,
            now,
        );
        // With lambda=0.01, w(100 days) ~ e^-1 ~ 0.368. So recent record
        // heavily outweighs old one.
        assert!(score.score > 0.65, "score was {}", score.score);
    }

    #[test]
    fn aggregate_trust_uses_all_task_types() {
        let engine = TrustEngine::default();
        let evaluator = AgentId::new();
        let subject = AgentId::new();
        let now = Utc::now();

        let records = vec![
            make_record(evaluator, subject, TaskType::CodeGeneration, 0.9, 0, now),
            make_record(evaluator, subject, TaskType::DataProcessing, 0.6, 0, now),
        ];

        let agg = engine.compute_aggregate_trust(subject, &records, now);
        // Weighted by complexity: CG=1.5, DP=0.8
        // (0.9*1.5 + 0.6*0.8) / (1.5 + 0.8) = (1.35 + 0.48) / 2.3 ~ 0.7957
        assert!((agg - 0.7957).abs() < 0.01);
    }

    #[test]
    fn trust_vector_populates_all_types() {
        let engine = TrustEngine::default();
        let subject = AgentId::new();
        let now = Utc::now();

        let tv = engine.compute_trust_vector(subject, &[], now);
        for &tt in TaskType::all() {
            assert!((tv.get(tt) - DEFAULT_PRIOR).abs() < 1e-9);
        }
    }

    #[test]
    fn decay_weight_at_zero_is_one() {
        let engine = TrustEngine::default();
        assert!((engine.decay_weight(0.0) - 1.0).abs() < 1e-12);
    }

    #[test]
    fn score_clamped_to_unit_interval() {
        let engine = TrustEngine::default();
        let evaluator = AgentId::new();
        let subject = AgentId::new();
        let now = Utc::now();

        // Quality score above 1.0 (edge case)
        let mut record = make_record(evaluator, subject, TaskType::Analysis, 1.5, 0, now);
        record.quality_score = 1.5;

        let score = engine.compute_trust_score(subject, TaskType::Analysis, &[record], now);
        assert!(score.score <= 1.0);
    }
}
