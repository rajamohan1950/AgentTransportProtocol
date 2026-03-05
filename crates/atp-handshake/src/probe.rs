//! Phase 1: CAPABILITY_PROBE creation and processing.
//!
//! A `CAPABILITY_PROBE` is broadcast by a task requester to discover agents
//! that can satisfy specific task-type requirements within QoS bounds.
//! Candidate agents evaluate the probe against their own capabilities and
//! decide whether to respond with a `CAPABILITY_OFFER`.

use atp_types::{
    AgentId, Capability, CapabilityProbeMsg, ContextEmbedding, QoSConstraints, TaskType,
};
use chrono::Utc;
use rand::Rng;

use crate::registry::{CapabilityRegistry, RegistryEntry};

/// Create a new CAPABILITY_PROBE message.
///
/// The probe advertises what the requester needs: a task type, QoS bounds,
/// and an optional context embedding for semantic matching. A random nonce
/// is generated to correlate offers with this specific probe.
///
/// The `signature` field is left empty (zero-length) and should be filled
/// by the caller using the agent's signing key before transmission.
pub fn create_probe(
    from: AgentId,
    task_type: TaskType,
    qos: QoSConstraints,
    context_embedding: Option<ContextEmbedding>,
) -> CapabilityProbeMsg {
    let mut rng = rand::thread_rng();
    CapabilityProbeMsg {
        from,
        task_type,
        qos,
        context_embedding,
        nonce: rng.gen(),
        timestamp: Utc::now(),
        signature: Vec::new(),
    }
}

/// Result of processing a probe against the local registry.
#[derive(Debug, Clone)]
pub struct ProbeResult {
    /// Agents whose capabilities satisfy the probe's QoS constraints.
    pub matching_entries: Vec<RegistryEntry>,
    /// The original probe nonce, for correlating offers.
    pub probe_nonce: u64,
    /// The task type requested.
    pub task_type: TaskType,
    /// The QoS constraints from the probe.
    pub qos: QoSConstraints,
}

/// Process an incoming CAPABILITY_PROBE against a capability registry.
///
/// Returns a [`ProbeResult`] containing all agents whose registered
/// capabilities meet or exceed the QoS constraints in the probe. The
/// caller should then have each matching agent generate a
/// `CAPABILITY_OFFER`.
pub fn process_probe(
    probe: &CapabilityProbeMsg,
    registry: &CapabilityRegistry,
) -> ProbeResult {
    let matching: Vec<RegistryEntry> = registry
        .find_capable(probe.task_type, &probe.qos)
        .into_iter()
        .cloned()
        .collect();

    tracing::debug!(
        task_type = %probe.task_type,
        nonce = probe.nonce,
        matches = matching.len(),
        "processed CAPABILITY_PROBE"
    );

    ProbeResult {
        matching_entries: matching,
        probe_nonce: probe.nonce,
        task_type: probe.task_type,
        qos: probe.qos.clone(),
    }
}

/// Evaluate whether a single capability satisfies a probe's QoS constraints
/// given a trust score. Useful for agents deciding locally whether to respond.
pub fn capability_matches_probe(
    capability: &Capability,
    trust_score: f64,
    qos: &QoSConstraints,
) -> bool {
    capability.estimated_quality >= qos.min_quality
        && capability.estimated_latency <= qos.max_latency
        && capability.cost_per_task <= qos.max_cost
        && trust_score >= qos.min_trust
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn make_cap(quality: f64, latency_ms: u64, cost: f64) -> Capability {
        Capability {
            task_type: TaskType::CodeGeneration,
            estimated_quality: quality,
            estimated_latency: Duration::from_millis(latency_ms),
            cost_per_task: cost,
        }
    }

    #[test]
    fn create_probe_populates_fields() {
        let from = AgentId::new();
        let qos = QoSConstraints::default();
        let probe = create_probe(from, TaskType::Analysis, qos, None);

        assert_eq!(probe.from, from);
        assert!(matches!(probe.task_type, TaskType::Analysis));
        assert!(probe.nonce != 0 || true); // nonce is random, could be 0
        assert!(probe.signature.is_empty());
    }

    #[test]
    fn process_probe_finds_matching_agents() {
        let mut registry = CapabilityRegistry::new();
        let a1 = AgentId::new();
        let a2 = AgentId::new();
        let a3 = AgentId::new();

        // a1: high quality, meets all constraints
        registry.register(a1, make_cap(0.9, 100, 0.5), 0.8);
        // a2: low quality, won't meet min_quality=0.7
        registry.register(a2, make_cap(0.5, 100, 0.5), 0.8);
        // a3: high quality but wrong task type
        registry.register(
            a3,
            Capability {
                task_type: TaskType::Analysis,
                estimated_quality: 0.9,
                estimated_latency: Duration::from_millis(100),
                cost_per_task: 0.5,
            },
            0.8,
        );

        let qos = QoSConstraints {
            min_quality: 0.7,
            max_latency: Duration::from_millis(500),
            max_cost: 1.0,
            min_trust: 0.5,
        };

        let probe = create_probe(AgentId::new(), TaskType::CodeGeneration, qos, None);
        let result = process_probe(&probe, &registry);

        assert_eq!(result.matching_entries.len(), 1);
        assert_eq!(result.matching_entries[0].agent_id, a1);
    }

    #[test]
    fn capability_matches_probe_checks_all_dimensions() {
        let qos = QoSConstraints {
            min_quality: 0.7,
            max_latency: Duration::from_millis(200),
            max_cost: 1.0,
            min_trust: 0.5,
        };

        // All good
        assert!(capability_matches_probe(&make_cap(0.8, 100, 0.5), 0.7, &qos));

        // Quality too low
        assert!(!capability_matches_probe(&make_cap(0.6, 100, 0.5), 0.7, &qos));

        // Latency too high
        assert!(!capability_matches_probe(&make_cap(0.8, 300, 0.5), 0.7, &qos));

        // Cost too high
        assert!(!capability_matches_probe(&make_cap(0.8, 100, 1.5), 0.7, &qos));

        // Trust too low
        assert!(!capability_matches_probe(&make_cap(0.8, 100, 0.5), 0.3, &qos));
    }
}
