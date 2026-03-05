//! Message dispatcher for ATP protocol messages.
//!
//! Routes incoming [`AtpMessage`] variants to the appropriate protocol
//! layer handler and returns optional response messages.

use atp_types::{
    AgentId, AtpError, AtpMessage, BackpressureMsg, CapabilityOfferMsg, CapabilityProbeMsg,
    CircuitBreakMsg, CircuitState, ContextRequestMsg, HeartbeatMsg, InteractionProofMsg,
    InteractionRecord, TaskResultMsg, TaskSubmitMsg,
};

use atp_fault::{
    AgentLoadTracker, CheckpointStore, CircuitBreaker, HeartbeatMonitor, PoisonDetector,
};
use atp_handshake::{create_offer, process_probe, CapabilityRegistry};
use atp_identity::IdentityStore;

use tracing::{debug, info, warn};

/// Dispatches incoming ATP messages to the correct protocol-layer handler.
///
/// The dispatcher holds references to all layer components and provides a
/// single `dispatch` method that pattern-matches on the message variant.
/// It is designed to be used inside [`AtpNode`](crate::node::AtpNode) but
/// can also be used standalone for testing.
pub struct MessageDispatcher {
    /// This node's agent ID (used for building response messages).
    node_id: AgentId,
}

impl MessageDispatcher {
    /// Create a new dispatcher for the given node identity.
    pub fn new(node_id: AgentId) -> Self {
        Self { node_id }
    }

    /// Dispatch a single incoming message to the appropriate layer handler.
    ///
    /// Returns an optional response message. Some message types (e.g.
    /// heartbeats, interaction proofs) are "fire and forget" and produce
    /// no response.
    #[allow(clippy::too_many_arguments)]
    pub async fn dispatch(
        &self,
        msg: AtpMessage,
        identity_store: &IdentityStore,
        registry: &CapabilityRegistry,
        heartbeat_monitor: &HeartbeatMonitor,
        circuit_breaker: &CircuitBreaker,
        _checkpoint_store: &CheckpointStore,
        poison_detector: &PoisonDetector,
        load_tracker: &AgentLoadTracker,
    ) -> Result<Option<AtpMessage>, AtpError> {
        match msg {
            AtpMessage::CapabilityProbe(probe) => {
                self.handle_probe(probe, registry).await
            }
            AtpMessage::CapabilityOffer(offer) => {
                self.handle_offer(offer).await
            }
            AtpMessage::ContractAccept(contract) => {
                self.handle_contract(contract).await
            }
            AtpMessage::ContextRequest(request) => {
                self.handle_context_request(request).await
            }
            AtpMessage::TaskSubmit(submit) => {
                self.handle_task_submit(submit, circuit_breaker, poison_detector)
                    .await
            }
            AtpMessage::TaskResult(result) => {
                self.handle_task_result(result, identity_store, circuit_breaker)
                    .await
            }
            AtpMessage::InteractionProof(proof) => {
                self.handle_interaction_proof(proof, identity_store).await
            }
            AtpMessage::Heartbeat(hb) => {
                self.handle_heartbeat(hb, heartbeat_monitor, load_tracker)
                    .await
            }
            AtpMessage::Backpressure(bp) => {
                self.handle_backpressure(bp, load_tracker).await
            }
            AtpMessage::CircuitBreak(cb) => {
                self.handle_circuit_break(cb, circuit_breaker).await
            }
        }
    }

    // ── Per-message-type handlers ────────────────────────────────────

    /// Handle an incoming CAPABILITY_PROBE.
    ///
    /// Checks the local registry for matching capabilities and responds
    /// with a CAPABILITY_OFFER if this node can serve the request.
    async fn handle_probe(
        &self,
        probe: CapabilityProbeMsg,
        registry: &CapabilityRegistry,
    ) -> Result<Option<AtpMessage>, AtpError> {
        debug!(
            from = %probe.from,
            task_type = %probe.task_type,
            nonce = probe.nonce,
            "received CAPABILITY_PROBE"
        );

        let result = process_probe(&probe, registry);

        // Check if *this node* is among the matching agents.
        let my_entry = result
            .matching_entries
            .iter()
            .find(|e| e.agent_id == self.node_id);

        if let Some(entry) = my_entry {
            let offer = create_offer(
                self.node_id,
                &probe,
                entry.capability.clone(),
                entry.trust_score,
                std::time::Duration::from_secs(5),
            );
            info!(
                to = %probe.from,
                quality = entry.capability.estimated_quality,
                "responding with CAPABILITY_OFFER"
            );
            Ok(Some(AtpMessage::CapabilityOffer(offer)))
        } else {
            debug!("no matching capability for probe, not responding");
            Ok(None)
        }
    }

    /// Handle an incoming CAPABILITY_OFFER.
    ///
    /// Offers are collected by the handshake coordinator during negotiation.
    /// When received outside active negotiation, they are logged and dropped.
    async fn handle_offer(
        &self,
        offer: CapabilityOfferMsg,
    ) -> Result<Option<AtpMessage>, AtpError> {
        debug!(
            from = %offer.from,
            quality = offer.capability.estimated_quality,
            trust = offer.trust_score,
            "received CAPABILITY_OFFER (queued for coordinator)"
        );
        // In a full implementation, offers would be pushed into the
        // HandshakeCoordinator's pending-offers channel. For the
        // composition root we acknowledge receipt without a response.
        Ok(None)
    }

    /// Handle an incoming CONTRACT_ACCEPT.
    async fn handle_contract(
        &self,
        contract: atp_types::ContractAcceptMsg,
    ) -> Result<Option<AtpMessage>, AtpError> {
        info!(
            from = %contract.from,
            contract_id = %contract.contract_id,
            "received CONTRACT_ACCEPT"
        );
        Ok(None)
    }

    /// Handle an incoming CONTEXT_REQUEST.
    async fn handle_context_request(
        &self,
        request: ContextRequestMsg,
    ) -> Result<Option<AtpMessage>, AtpError> {
        debug!(
            from = %request.from,
            task_id = %request.task_id,
            chunks_requested = request.requested_chunk_indices.len(),
            "received CONTEXT_REQUEST"
        );
        // Context request fulfillment is handled at the node level where
        // the original context data is available. The dispatcher signals
        // the node by returning None; the node checks and fulfills.
        Ok(None)
    }

    /// Handle an incoming TASK_SUBMIT.
    ///
    /// Checks circuit breaker and poison status before accepting the task.
    async fn handle_task_submit(
        &self,
        submit: TaskSubmitMsg,
        circuit_breaker: &CircuitBreaker,
        poison_detector: &PoisonDetector,
    ) -> Result<Option<AtpMessage>, AtpError> {
        info!(
            from = %submit.from,
            task_id = %submit.task_id,
            task_type = %submit.task_type,
            "received TASK_SUBMIT"
        );

        // Check if the submitting agent's circuit is open.
        if let Err(e) = circuit_breaker.allow_request(&submit.from) {
            warn!(from = %submit.from, "rejecting task: circuit open");
            return Err(AtpError::Fault(e));
        }

        // Check if the task is poisoned.
        if poison_detector.is_poisoned(&submit.task_id) {
            warn!(task_id = %submit.task_id, "rejecting task: poisoned");
            return Err(AtpError::Fault(
                atp_types::FaultError::PoisonTask(submit.task_id.to_string()),
            ));
        }

        // Task execution would be handled by the node's task executor.
        // Returning None signals that the task was accepted for processing.
        Ok(None)
    }

    /// Handle an incoming TASK_RESULT.
    ///
    /// Records success with the circuit breaker and generates an
    /// interaction proof.
    async fn handle_task_result(
        &self,
        result: TaskResultMsg,
        identity_store: &IdentityStore,
        circuit_breaker: &CircuitBreaker,
    ) -> Result<Option<AtpMessage>, AtpError> {
        info!(
            from = %result.from,
            task_id = %result.task_id,
            quality = result.quality_self_report,
            elapsed_ms = result.elapsed.as_millis() as u64,
            cost = result.actual_cost,
            "received TASK_RESULT"
        );

        // Record success for circuit breaker.
        circuit_breaker.record_success(&result.from);

        // Generate an interaction proof for the trust system.
        let proof = InteractionProofMsg {
            evaluator: self.node_id,
            subject: result.from,
            task_id: result.task_id,
            task_type: atp_types::TaskType::CodeGeneration, // will be overridden by caller context
            quality_score: result.quality_self_report,
            latency_ms: result.elapsed.as_millis() as u64,
            cost: result.actual_cost,
            timestamp: chrono::Utc::now(),
            signature: Vec::new(),
        };

        // Record the interaction in the identity store.
        let record = InteractionRecord {
            evaluator: proof.evaluator,
            subject: proof.subject,
            task_type: proof.task_type,
            quality_score: proof.quality_score,
            latency_ms: proof.latency_ms,
            cost: proof.cost,
            timestamp: proof.timestamp,
            signature: proof.signature.clone(),
        };
        identity_store.add_interaction(record).await;

        Ok(Some(AtpMessage::InteractionProof(proof)))
    }

    /// Handle an incoming INTERACTION_PROOF.
    ///
    /// Records the interaction for trust computation.
    async fn handle_interaction_proof(
        &self,
        proof: InteractionProofMsg,
        identity_store: &IdentityStore,
    ) -> Result<Option<AtpMessage>, AtpError> {
        debug!(
            evaluator = %proof.evaluator,
            subject = %proof.subject,
            quality = proof.quality_score,
            "received INTERACTION_PROOF"
        );

        let record = InteractionRecord {
            evaluator: proof.evaluator,
            subject: proof.subject,
            task_type: proof.task_type,
            quality_score: proof.quality_score,
            latency_ms: proof.latency_ms,
            cost: proof.cost,
            timestamp: proof.timestamp,
            signature: proof.signature,
        };
        identity_store.add_interaction(record).await;

        Ok(None)
    }

    /// Handle an incoming HEARTBEAT.
    ///
    /// Updates both the heartbeat monitor and the load tracker.
    async fn handle_heartbeat(
        &self,
        hb: HeartbeatMsg,
        heartbeat_monitor: &HeartbeatMonitor,
        load_tracker: &AgentLoadTracker,
    ) -> Result<Option<AtpMessage>, AtpError> {
        debug!(
            from = %hb.from,
            seq = hb.sequence,
            queue_depth = hb.queue_depth,
            load = hb.load_factor,
            "received HEARTBEAT"
        );

        heartbeat_monitor.record_heartbeat(&hb);
        load_tracker.update_from_heartbeat(&hb);

        // Check if the agent is overloaded and generate backpressure.
        if load_tracker.is_overloaded(&hb.from) {
            let bp_msg = load_tracker.build_message(hb.from);
            return Ok(Some(AtpMessage::Backpressure(bp_msg)));
        }

        Ok(None)
    }

    /// Handle an incoming BACKPRESSURE signal.
    async fn handle_backpressure(
        &self,
        bp: BackpressureMsg,
        load_tracker: &AgentLoadTracker,
    ) -> Result<Option<AtpMessage>, AtpError> {
        warn!(
            from = %bp.from,
            queue_depth = bp.queue_depth,
            recommended_rate = bp.recommended_rate,
            drain_ms = bp.estimated_drain_ms,
            "received BACKPRESSURE signal"
        );

        load_tracker.update_from_backpressure(&bp);
        Ok(None)
    }

    /// Handle an incoming CIRCUIT_BREAK notification.
    async fn handle_circuit_break(
        &self,
        cb: CircuitBreakMsg,
        circuit_breaker: &CircuitBreaker,
    ) -> Result<Option<AtpMessage>, AtpError> {
        warn!(
            from = %cb.from,
            target = %cb.target,
            state = ?cb.state,
            failures = cb.failure_count,
            "received CIRCUIT_BREAK notification"
        );

        // If a peer tells us their circuit to a target is open, we record
        // failures against that target as well (protocol-level gossip).
        match cb.state {
            CircuitState::Open => {
                circuit_breaker.record_failure(&cb.target);
            }
            CircuitState::Closed => {
                circuit_breaker.record_success(&cb.target);
            }
            CircuitState::HalfOpen => {
                // Informational only -- no action needed.
            }
        }

        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use atp_types::{AgentId, Capability, HeartbeatMsg, QoSConstraints, TaskType};
    use std::time::Duration;

    fn make_cap(task_type: TaskType, quality: f64, latency_ms: u64, cost: f64) -> Capability {
        Capability {
            task_type,
            estimated_quality: quality,
            estimated_latency: Duration::from_millis(latency_ms),
            cost_per_task: cost,
        }
    }

    fn default_components(
        _node_id: AgentId,
    ) -> (
        IdentityStore,
        CapabilityRegistry,
        HeartbeatMonitor,
        CircuitBreaker,
        CheckpointStore,
        PoisonDetector,
        AgentLoadTracker,
    ) {
        (
            IdentityStore::new(),
            CapabilityRegistry::new(),
            HeartbeatMonitor::with_defaults(),
            CircuitBreaker::with_defaults(),
            CheckpointStore::new(),
            PoisonDetector::with_defaults(),
            AgentLoadTracker::with_defaults(),
        )
    }

    #[tokio::test]
    async fn test_dispatch_heartbeat() {
        let node_id = AgentId::new();
        let dispatcher = MessageDispatcher::new(node_id);
        let (id_store, registry, hb_mon, cb, cp, pd, lt) = default_components(node_id);

        let sender = AgentId::new();
        let msg = AtpMessage::Heartbeat(HeartbeatMsg {
            from: sender,
            sequence: 1,
            queue_depth: 5,
            load_factor: 0.3,
        });

        let result = dispatcher
            .dispatch(msg, &id_store, &registry, &hb_mon, &cb, &cp, &pd, &lt)
            .await;

        assert!(result.is_ok());
        // No backpressure expected for low queue depth.
        assert!(result.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_dispatch_probe_with_capability() {
        let node_id = AgentId::new();
        let dispatcher = MessageDispatcher::new(node_id);
        let (id_store, mut registry, hb_mon, cb, cp, pd, lt) = default_components(node_id);

        // Register this node's capability.
        registry.register(
            node_id,
            make_cap(TaskType::CodeGeneration, 0.9, 100, 0.5),
            0.85,
        );

        let probe_msg = atp_handshake::create_probe(
            AgentId::new(),
            TaskType::CodeGeneration,
            QoSConstraints {
                min_quality: 0.7,
                max_latency: Duration::from_secs(1),
                max_cost: 1.0,
                min_trust: 0.5,
            },
            None,
        );

        let msg = AtpMessage::CapabilityProbe(probe_msg);
        let result = dispatcher
            .dispatch(msg, &id_store, &registry, &hb_mon, &cb, &cp, &pd, &lt)
            .await;

        assert!(result.is_ok());
        let response = result.unwrap();
        assert!(response.is_some());
        assert!(matches!(response.unwrap(), AtpMessage::CapabilityOffer(_)));
    }

    #[tokio::test]
    async fn test_dispatch_interaction_proof() {
        let node_id = AgentId::new();
        let dispatcher = MessageDispatcher::new(node_id);
        let (id_store, registry, hb_mon, cb, cp, pd, lt) = default_components(node_id);

        let subject = AgentId::new();
        let proof = InteractionProofMsg {
            evaluator: AgentId::new(),
            subject,
            task_id: uuid::Uuid::new_v4(),
            task_type: TaskType::Analysis,
            quality_score: 0.88,
            latency_ms: 150,
            cost: 0.3,
            timestamp: chrono::Utc::now(),
            signature: Vec::new(),
        };

        let msg = AtpMessage::InteractionProof(proof);
        let result = dispatcher
            .dispatch(msg, &id_store, &registry, &hb_mon, &cb, &cp, &pd, &lt)
            .await;

        assert!(result.is_ok());
        assert!(result.unwrap().is_none());

        // Verify the interaction was recorded.
        let interactions = id_store.get_interactions(&subject).await;
        assert_eq!(interactions.len(), 1);
        assert!((interactions[0].quality_score - 0.88).abs() < f64::EPSILON);
    }

    #[tokio::test]
    async fn test_dispatch_task_submit_circuit_open() {
        let node_id = AgentId::new();
        let dispatcher = MessageDispatcher::new(node_id);
        let (id_store, registry, hb_mon, cb, cp, pd, lt) = default_components(node_id);

        let bad_agent = AgentId::new();

        // Trip the circuit breaker for bad_agent.
        for _ in 0..3 {
            cb.record_failure(&bad_agent);
        }

        let submit = TaskSubmitMsg {
            from: bad_agent,
            to: node_id,
            task_id: uuid::Uuid::new_v4(),
            task_type: TaskType::Analysis,
            payload: vec![1, 2, 3],
            context: atp_types::ContextDiff {
                base_hash: [0u8; 32],
                chunks: vec![],
                confidence: 0.9,
                original_size: 0,
                compressed_size: 0,
            },
            contract_id: uuid::Uuid::new_v4(),
        };

        let msg = AtpMessage::TaskSubmit(submit);
        let result = dispatcher
            .dispatch(msg, &id_store, &registry, &hb_mon, &cb, &cp, &pd, &lt)
            .await;

        assert!(result.is_err());
    }
}
