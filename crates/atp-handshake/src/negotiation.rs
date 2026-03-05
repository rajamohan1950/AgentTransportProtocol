//! Full 3-phase handshake orchestration with timeout and QoS relaxation.
//!
//! The [`HandshakeCoordinator`] drives the complete probe -> offer -> contract
//! lifecycle.  On each attempt it:
//!
//! 1. Broadcasts a `CAPABILITY_PROBE` and waits up to the configured timeout
//!    (default 500 ms) for offers.
//! 2. Ranks incoming offers using a weighted composite score.
//! 3. Selects the best offer and issues a `CONTRACT_ACCEPT`.
//!
//! If no offers are received within the timeout, the coordinator relaxes QoS
//! constraints by 10 % and retries (up to 3 retries by default).

use std::time::Duration;

use atp_types::{
    AgentId, CapabilityOfferMsg, ContractAcceptMsg,
    HandshakeError, HandshakeConfig, QoSConstraints, TaskType,
};

use crate::contract::{create_contract, HandshakeState, HandshakeStateMachine};
use crate::offer::{is_offer_expired, rank_offers, RankedOffer};
use crate::probe::{create_probe, process_probe};
use crate::registry::CapabilityRegistry;

/// The successful outcome of a handshake negotiation.
#[derive(Debug, Clone)]
pub struct HandshakeOutcome {
    /// The CONTRACT_ACCEPT message sent to the winning agent.
    pub contract: ContractAcceptMsg,
    /// The ranked offer that was selected.
    pub selected_offer: RankedOffer,
    /// How many attempts were needed (1 = first try succeeded).
    pub attempts: u32,
    /// The (possibly relaxed) QoS that was used for the successful probe.
    pub effective_qos: QoSConstraints,
}

/// Orchestrates the full 3-phase handshake flow.
///
/// In a real deployment the coordinator would use network I/O (gRPC streams)
/// to broadcast probes and collect offers.  This implementation works against
/// a local [`CapabilityRegistry`] so that the handshake logic can be tested
/// and benchmarked without a transport layer.
#[derive(Debug)]
pub struct HandshakeCoordinator {
    /// The requester's agent ID.
    requester: AgentId,
    /// Handshake configuration (timeout, relaxation, max retries).
    config: HandshakeConfig,
    /// State machine tracking the handshake lifecycle.
    state_machine: HandshakeStateMachine,
    /// Duration for which an accepted contract is valid.
    contract_duration: Duration,
}

impl HandshakeCoordinator {
    /// Create a new coordinator for the given requester.
    pub fn new(requester: AgentId, config: HandshakeConfig) -> Self {
        Self {
            requester,
            config,
            state_machine: HandshakeStateMachine::new(),
            contract_duration: Duration::from_secs(60),
        }
    }

    /// Create a coordinator with default configuration.
    pub fn with_defaults(requester: AgentId) -> Self {
        Self::new(requester, HandshakeConfig::default())
    }

    /// Override the contract duration (default 60 s).
    pub fn set_contract_duration(&mut self, duration: Duration) {
        self.contract_duration = duration;
    }

    /// Current state of the handshake.
    pub fn state(&self) -> &HandshakeState {
        self.state_machine.state()
    }

    /// Run the full handshake against a local registry.
    ///
    /// This is the primary entry point.  It performs the three phases
    /// synchronously (no actual network I/O), with retry/relaxation if
    /// no matching agents are found.
    pub fn negotiate(
        &mut self,
        task_type: TaskType,
        qos: &QoSConstraints,
        registry: &CapabilityRegistry,
    ) -> Result<HandshakeOutcome, HandshakeError> {
        let mut current_qos = qos.clone();
        let max_attempts = self.config.max_retries + 1;

        for attempt in 1..=max_attempts {
            tracing::info!(
                attempt,
                max_attempts,
                task_type = %task_type,
                min_quality = current_qos.min_quality,
                max_latency_ms = current_qos.max_latency.as_millis() as u64,
                max_cost = current_qos.max_cost,
                min_trust = current_qos.min_trust,
                "handshake attempt"
            );

            // Ensure we are in Idle before starting.
            if *self.state_machine.state() != HandshakeState::Idle {
                self.state_machine.reset_for_retry().map_err(|_| {
                    HandshakeError::InvalidTransition {
                        from: self.state_machine.state().to_string(),
                        to: "Idle".to_string(),
                    }
                })?;
            }

            // -- Phase 1: CAPABILITY_PROBE --
            let probe = create_probe(
                self.requester,
                task_type,
                current_qos.clone(),
                None,
            );
            self.state_machine.on_probe_sent()?;

            let probe_result = process_probe(&probe, registry);

            if probe_result.matching_entries.is_empty() {
                tracing::warn!(
                    attempt,
                    task_type = %task_type,
                    "no capable agents found, will relax QoS"
                );

                // If more retries remain, relax and try again.
                if attempt < max_attempts {
                    self.state_machine.reset_for_retry()?;
                    current_qos = current_qos.relax(self.config.relaxation_factor);
                    continue;
                } else {
                    self.state_machine.on_failure()?;
                    return Err(HandshakeError::NoCapableAgents(task_type.to_string()));
                }
            }

            // -- Phase 2: CAPABILITY_OFFER (simulated) --
            // In a local-registry flow we synthesise offers from the
            // matching registry entries.
            let offers: Vec<CapabilityOfferMsg> = probe_result
                .matching_entries
                .iter()
                .map(|entry| {
                    crate::offer::create_offer(
                        entry.agent_id,
                        &probe,
                        entry.capability.clone(),
                        entry.trust_score,
                        crate::offer::DEFAULT_OFFER_TTL,
                    )
                })
                .collect();

            // Filter out expired offers (unlikely in local flow, but correct).
            let valid_offers: Vec<CapabilityOfferMsg> = offers
                .into_iter()
                .filter(|o| !is_offer_expired(o))
                .collect();

            if valid_offers.is_empty() {
                tracing::warn!("all offers expired");
                if attempt < max_attempts {
                    self.state_machine.reset_for_retry()?;
                    current_qos = current_qos.relax(self.config.relaxation_factor);
                    continue;
                } else {
                    self.state_machine.on_failure()?;
                    return Err(HandshakeError::NegotiationFailed(
                        "all offers expired".to_string(),
                    ));
                }
            }

            self.state_machine.on_offers_received()?;

            // Rank the offers.
            let ranked = rank_offers(
                &valid_offers,
                current_qos.max_latency,
                current_qos.max_cost,
            );

            let best = ranked.into_iter().next().ok_or_else(|| {
                HandshakeError::NegotiationFailed("no ranked offers".to_string())
            })?;

            tracing::info!(
                agent = %best.offer.from,
                score = best.score,
                quality = best.offer.capability.estimated_quality,
                latency_ms = best.offer.capability.estimated_latency.as_millis() as u64,
                cost = best.offer.capability.cost_per_task,
                trust = best.offer.trust_score,
                "selected best offer"
            );

            // -- Phase 3: CONTRACT_ACCEPT --
            let contract = create_contract(
                self.requester,
                &best,
                &current_qos,
                self.contract_duration,
            );

            self.state_machine
                .on_contract_accepted(contract.contract_id, best.offer.from)?;

            return Ok(HandshakeOutcome {
                contract,
                selected_offer: best,
                attempts: attempt,
                effective_qos: current_qos,
            });
        }

        // Should be unreachable, but handle gracefully.
        self.state_machine.on_failure().ok();
        Err(HandshakeError::NegotiationFailed(
            "exhausted all attempts".to_string(),
        ))
    }

    /// Run the handshake asynchronously with a per-phase timeout.
    ///
    /// This wraps [`negotiate`](Self::negotiate) in a `tokio::time::timeout`
    /// to enforce the configured probe timeout as an overall deadline.
    pub async fn negotiate_async(
        &mut self,
        task_type: TaskType,
        qos: &QoSConstraints,
        registry: &CapabilityRegistry,
    ) -> Result<HandshakeOutcome, HandshakeError> {
        let timeout_dur = self.config.probe_timeout * (self.config.max_retries + 1);

        let result = tokio::time::timeout(timeout_dur, async {
            // The actual negotiation is CPU-bound; run it directly.
            self.negotiate(task_type, qos, registry)
        })
        .await;

        match result {
            Ok(inner) => inner,
            Err(_) => {
                self.state_machine.on_failure().ok();
                Err(HandshakeError::Timeout(timeout_dur))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use atp_types::Capability;

    fn make_cap(task_type: TaskType, quality: f64, latency_ms: u64, cost: f64) -> Capability {
        Capability {
            task_type,
            estimated_quality: quality,
            estimated_latency: Duration::from_millis(latency_ms),
            cost_per_task: cost,
        }
    }

    fn populated_registry() -> CapabilityRegistry {
        let mut reg = CapabilityRegistry::new();

        let a1 = AgentId::new();
        let a2 = AgentId::new();
        let a3 = AgentId::new();

        reg.register(a1, make_cap(TaskType::CodeGeneration, 0.9, 100, 0.5), 0.85);
        reg.register(a2, make_cap(TaskType::CodeGeneration, 0.8, 150, 0.4), 0.75);
        reg.register(a3, make_cap(TaskType::Analysis, 0.95, 50, 0.3), 0.9);

        reg
    }

    #[test]
    fn happy_path_negotiation() {
        let registry = populated_registry();
        let requester = AgentId::new();
        let mut coordinator = HandshakeCoordinator::with_defaults(requester);

        let qos = QoSConstraints {
            min_quality: 0.7,
            max_latency: Duration::from_secs(1),
            max_cost: 1.0,
            min_trust: 0.5,
        };

        let outcome = coordinator
            .negotiate(TaskType::CodeGeneration, &qos, &registry)
            .unwrap();

        assert_eq!(*coordinator.state(), HandshakeState::Contracted);
        assert_eq!(outcome.attempts, 1);
        assert!(outcome.selected_offer.score > 0.0);
        assert_eq!(outcome.contract.from, requester);
    }

    #[test]
    fn negotiation_with_relaxation() {
        let mut registry = CapabilityRegistry::new();
        let agent = AgentId::new();
        // Agent has quality 0.65 -- below the strict 0.7 threshold, but
        // after one 10% relaxation (0.7 * 0.9 = 0.63) it should match.
        registry.register(
            agent,
            make_cap(TaskType::DataProcessing, 0.65, 100, 0.5),
            0.6,
        );

        let requester = AgentId::new();
        let mut coordinator = HandshakeCoordinator::with_defaults(requester);

        let qos = QoSConstraints {
            min_quality: 0.7,
            max_latency: Duration::from_secs(1),
            max_cost: 1.0,
            min_trust: 0.5,
        };

        let outcome = coordinator
            .negotiate(TaskType::DataProcessing, &qos, &registry)
            .unwrap();

        assert_eq!(outcome.attempts, 2); // First attempt fails, second with relaxed QoS succeeds
        assert_eq!(outcome.contract.to, agent);
    }

    #[test]
    fn negotiation_fails_no_agents() {
        let registry = CapabilityRegistry::new(); // empty
        let requester = AgentId::new();
        let mut coordinator = HandshakeCoordinator::with_defaults(requester);

        let qos = QoSConstraints::default();
        let result = coordinator.negotiate(TaskType::Analysis, &qos, &registry);

        assert!(result.is_err());
        assert_eq!(*coordinator.state(), HandshakeState::Failed);

        match result.unwrap_err() {
            HandshakeError::NoCapableAgents(tt) => assert_eq!(tt, "analysis"),
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn negotiation_fails_after_max_retries() {
        let mut registry = CapabilityRegistry::new();
        let agent = AgentId::new();
        // Quality so low that even 3 relaxations won't help:
        // 0.7 -> 0.63 -> 0.567 -> 0.51; agent has 0.1
        registry.register(
            agent,
            make_cap(TaskType::Analysis, 0.1, 100, 0.5),
            0.8,
        );

        let requester = AgentId::new();
        let mut coordinator = HandshakeCoordinator::with_defaults(requester);

        let qos = QoSConstraints {
            min_quality: 0.7,
            max_latency: Duration::from_secs(1),
            max_cost: 1.0,
            min_trust: 0.5,
        };

        let result = coordinator.negotiate(TaskType::Analysis, &qos, &registry);
        assert!(result.is_err());
        assert_eq!(*coordinator.state(), HandshakeState::Failed);
    }

    #[tokio::test]
    async fn async_negotiation_works() {
        let registry = populated_registry();
        let requester = AgentId::new();
        let mut coordinator = HandshakeCoordinator::with_defaults(requester);

        let qos = QoSConstraints {
            min_quality: 0.7,
            max_latency: Duration::from_secs(1),
            max_cost: 1.0,
            min_trust: 0.5,
        };

        let outcome = coordinator
            .negotiate_async(TaskType::CodeGeneration, &qos, &registry)
            .await
            .unwrap();

        assert_eq!(*coordinator.state(), HandshakeState::Contracted);
        assert_eq!(outcome.attempts, 1);
    }

    #[test]
    fn selects_best_offer_by_composite_score() {
        let mut registry = CapabilityRegistry::new();
        let good_agent = AgentId::new();
        let mediocre_agent = AgentId::new();

        // Good: high quality, low latency, low cost, high trust
        registry.register(
            good_agent,
            make_cap(TaskType::CreativeWriting, 0.95, 50, 0.2),
            0.9,
        );
        // Mediocre: meets constraints but lower on every metric
        registry.register(
            mediocre_agent,
            make_cap(TaskType::CreativeWriting, 0.72, 400, 0.8),
            0.55,
        );

        let requester = AgentId::new();
        let mut coordinator = HandshakeCoordinator::with_defaults(requester);

        let qos = QoSConstraints {
            min_quality: 0.7,
            max_latency: Duration::from_secs(1),
            max_cost: 1.0,
            min_trust: 0.5,
        };

        let outcome = coordinator
            .negotiate(TaskType::CreativeWriting, &qos, &registry)
            .unwrap();

        // The good agent should be selected.
        assert_eq!(outcome.contract.to, good_agent);
    }
}
