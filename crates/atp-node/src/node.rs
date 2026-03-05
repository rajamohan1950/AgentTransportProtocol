//! [`AtpNode`] — Composition root that wires all five protocol layers
//! into a single coherent agent node.
//!
//! The node orchestrates the full lifecycle of task execution:
//!
//! 1. **Identity (L1)** — Agent registration, trust queries
//! 2. **Handshake (L2)** — Capability discovery and contract negotiation
//! 3. **Context (L3)** — Semantic context compression / adaptive requests
//! 4. **Routing (L4)** — Multi-objective route selection
//! 5. **Fault (L5)** — Heartbeat, circuit breaker, checkpoint, poison

use std::time::{Duration, Instant};

use atp_context::ContextCompressor;
use atp_fault::{
    AgentLoadTracker, CheckpointStore, CircuitBreaker, HeartbeatMonitor, PoisonDetector,
    PoisonStatus,
};
use atp_handshake::{CapabilityRegistry, HandshakeCoordinator};
use atp_identity::IdentityStore;
use atp_routing::EconomicRouter;
use atp_types::{
    AgentId, AgentIdentity, AtpConfig, AtpError, AtpMessage, Capability, ContextDiff,
    FaultError, QoSConstraints, TaskResultMsg, TaskType,
};

use tracing::{debug, info, instrument, warn};

use crate::dispatcher::MessageDispatcher;

/// The full ATP agent node — composition root for all protocol layers.
///
/// `AtpNode` is designed to be created via [`AtpNodeBuilder`](crate::builder::AtpNodeBuilder)
/// or [`AtpNode::new`] with a configuration struct.
pub struct AtpNode {
    /// This node's unique agent identifier.
    agent_id: AgentId,
    /// Full configuration.
    config: AtpConfig,

    // ── Layer 1: Identity & Trust ────────────────────────────────────
    identity_store: IdentityStore,

    // ── Layer 2: Capability Handshake ────────────────────────────────
    capability_registry: CapabilityRegistry,
    handshake_coordinator: HandshakeCoordinator,

    // ── Layer 3: Semantic Context Differentials ──────────────────────
    context_compressor: ContextCompressor,

    // ── Layer 4: Economic Routing ────────────────────────────────────
    economic_router: EconomicRouter,

    // ── Layer 5: Fault Tolerance ─────────────────────────────────────
    heartbeat_monitor: HeartbeatMonitor,
    circuit_breaker: CircuitBreaker,
    checkpoint_store: CheckpointStore,
    poison_detector: PoisonDetector,
    load_tracker: AgentLoadTracker,

    // ── Message Dispatcher ───────────────────────────────────────────
    dispatcher: MessageDispatcher,
}

impl AtpNode {
    /// Create a new node with default layer implementations derived from
    /// the given configuration.
    pub fn new(config: AtpConfig) -> Self {
        let agent_id = AgentId::new();

        let identity_store = IdentityStore::new();
        let capability_registry = CapabilityRegistry::new();
        let handshake_coordinator =
            HandshakeCoordinator::new(agent_id, config.handshake.clone());

        let msc_config = atp_context::MscConfig {
            relevance_threshold: config.context.relevance_threshold,
            max_chunks: 10,
            chunk_size: 512,
            dimensions: config.context.embedding_dimensions,
        };
        let context_compressor = ContextCompressor::with_config(msc_config);

        let economic_router = EconomicRouter::with_config(
            atp_routing::AgentGraph::new(),
            atp_routing::CostModel::default(),
            config.routing.clone(),
        );

        let heartbeat_monitor = HeartbeatMonitor::new(config.fault.clone());
        let cooldown = config.fault.heartbeat_interval
            * config.fault.heartbeat_timeout_multiplier
            * 10;
        let circuit_breaker = CircuitBreaker::new(config.fault.clone(), cooldown);
        let checkpoint_store = CheckpointStore::new();
        let poison_detector = PoisonDetector::new(config.fault.clone());
        let load_tracker = AgentLoadTracker::new(config.fault.clone());

        let dispatcher = MessageDispatcher::new(agent_id);

        Self {
            agent_id,
            config,
            identity_store,
            capability_registry,
            handshake_coordinator,
            context_compressor,
            economic_router,
            heartbeat_monitor,
            circuit_breaker,
            checkpoint_store,
            poison_detector,
            load_tracker,
            dispatcher,
        }
    }

    /// Construct a node from pre-built components (used by the builder).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn from_components(
        agent_id: AgentId,
        config: AtpConfig,
        identity_store: IdentityStore,
        capability_registry: CapabilityRegistry,
        handshake_coordinator: HandshakeCoordinator,
        context_compressor: ContextCompressor,
        economic_router: EconomicRouter,
        heartbeat_monitor: HeartbeatMonitor,
        circuit_breaker: CircuitBreaker,
        checkpoint_store: CheckpointStore,
        poison_detector: PoisonDetector,
        load_tracker: AgentLoadTracker,
    ) -> Self {
        let dispatcher = MessageDispatcher::new(agent_id);
        Self {
            agent_id,
            config,
            identity_store,
            capability_registry,
            handshake_coordinator,
            context_compressor,
            economic_router,
            heartbeat_monitor,
            circuit_breaker,
            checkpoint_store,
            poison_detector,
            load_tracker,
            dispatcher,
        }
    }

    // ── Identity ─────────────────────────────────────────────────────

    /// Return this node's agent identifier.
    pub fn id(&self) -> AgentId {
        self.agent_id
    }

    /// Return a reference to the node's configuration.
    pub fn config(&self) -> &AtpConfig {
        &self.config
    }

    /// Register a capability for this node in the local registry.
    ///
    /// This is a convenience method that registers the capability under
    /// this node's own agent ID with the given trust score.
    pub fn register_capability(&mut self, cap: Capability, trust: f64) {
        self.capability_registry
            .register(self.agent_id, cap, trust);
    }

    /// Register an agent identity in the local identity store.
    ///
    /// Also registers the agent's capabilities in the handshake registry
    /// and adds it to the routing graph.
    #[instrument(skip(self, identity), fields(agent_id = %identity.id))]
    pub async fn register_agent(&mut self, identity: AgentIdentity) -> Result<(), AtpError> {
        let agent_id = identity.id;
        let capabilities = identity.capabilities.clone();

        // L1: Identity store
        self.identity_store
            .register(identity)
            .await
            .map_err(AtpError::Identity)?;

        // Compute initial trust (prior = 0.5 for new agents).
        let trust = self
            .identity_store
            .aggregate_trust(agent_id, chrono::Utc::now())
            .await;

        // L2: Capability registry
        for cap in &capabilities {
            self.capability_registry
                .register(agent_id, cap.clone(), trust);
        }

        // L4: Routing graph
        self.economic_router
            .add_agent(agent_id, capabilities.clone(), trust);

        // Connect new agent to all existing agents with default transfer latency.
        let existing_agents = self.identity_store.list_agents().await;
        for &existing in &existing_agents {
            if existing != agent_id {
                self.economic_router
                    .connect(agent_id, existing, Duration::from_millis(5));
            }
        }

        info!(
            agent = %agent_id,
            capabilities = capabilities.len(),
            trust = trust,
            "agent registered across all layers"
        );

        Ok(())
    }

    // ── Full Protocol Task Execution ─────────────────────────────────

    /// Execute a task through the full 5-layer ATP protocol.
    ///
    /// This is the primary high-level API. It drives:
    ///
    /// 1. **L5** — Pre-flight fault checks (circuit breaker, poison)
    /// 2. **L2** — Three-phase capability handshake (probe -> offer -> contract)
    /// 3. **L3** — Context compression via Semantic Context Differentials
    /// 4. **L4** — Economic route selection
    /// 5. **L5** — Checkpoint creation and interaction proof
    /// 6. **L1** — Trust score update from interaction
    ///
    /// Returns a simulated `TaskResultMsg`. In a real deployment the task
    /// payload would be sent over the transport layer to the contracted
    /// agent and the result received asynchronously.
    #[instrument(skip(self, payload), fields(task_type = %task_type))]
    pub async fn execute_task(
        &mut self,
        task_type: TaskType,
        payload: Vec<u8>,
        qos: QoSConstraints,
    ) -> Result<TaskResultMsg, AtpError> {
        let task_id = uuid::Uuid::new_v4();
        let start = Instant::now();

        info!(
            task_id = %task_id,
            task_type = %task_type,
            "beginning task execution"
        );

        // ── Step 1: L5 fault pre-checks ─────────────────────────────

        // Check poison status.
        if self.poison_detector.is_poisoned(&task_id) {
            return Err(AtpError::Fault(FaultError::PoisonTask(
                task_id.to_string(),
            )));
        }

        // ── Step 2: L2 capability handshake ─────────────────────────

        // Create a fresh coordinator per task to avoid state machine conflicts.
        let mut coordinator = HandshakeCoordinator::new(
            self.agent_id,
            self.config.handshake.clone(),
        );
        let handshake_outcome = coordinator
            .negotiate(task_type, &qos, &self.capability_registry)
            .map_err(AtpError::Handshake)?;

        let contracted_agent = handshake_outcome.contract.to;
        let contract_id = handshake_outcome.contract.contract_id;

        info!(
            agent = %contracted_agent,
            contract_id = %contract_id,
            attempts = handshake_outcome.attempts,
            "handshake complete"
        );

        // ── Step 3: L5 circuit breaker check on contracted agent ────

        if let Err(e) = self.circuit_breaker.allow_request(&contracted_agent) {
            warn!(
                agent = %contracted_agent,
                "contracted agent's circuit is open"
            );
            return Err(AtpError::Fault(e));
        }

        // ── Step 4: L3 context compression ──────────────────────────

        let context_diff = if !payload.is_empty() {
            self.context_compressor
                .compress_for_task(&payload, task_type, &payload)
                .map_err(AtpError::Context)?
        } else {
            ContextDiff {
                base_hash: [0u8; 32],
                chunks: vec![],
                confidence: 1.0,
                original_size: 0,
                compressed_size: 0,
            }
        };

        let compression_ratio = if context_diff.compressed_size > 0 {
            context_diff.original_size as f64 / context_diff.compressed_size as f64
        } else {
            1.0
        };

        debug!(
            original = context_diff.original_size,
            compressed = context_diff.compressed_size,
            ratio = format!("{:.1}x", compression_ratio),
            confidence = context_diff.confidence,
            "context compressed"
        );

        // ── Step 5: L4 route selection (informational) ──────────────

        let route_result = self.economic_router.find_route(task_type, &qos, None);
        if let Ok(ref route) = route_result {
            debug!(
                pattern = %route.pattern,
                hops = route.agents.len(),
                quality = route.metrics.quality,
                latency_ms = route.metrics.latency.as_millis() as u64,
                cost = route.metrics.cost,
                "route selected"
            );
        } else {
            debug!("no multi-hop route found; using direct contract");
        }

        // ── Step 6: L5 checkpoint creation ──────────────────────────

        self.checkpoint_store.create_checkpoint(
            task_id,
            payload.clone(),
            self.agent_id,
            1,
        );

        // ── Step 7: Simulate task execution ─────────────────────────
        //
        // In a real deployment, the TaskSubmitMsg would be sent over the
        // transport layer. Here we simulate the result based on the
        // contracted agent's advertised capability.

        let capability = self
            .capability_registry
            .get_capability(&contracted_agent, task_type);

        let (quality, cost) = match capability {
            Some(entry) => (
                entry.capability.estimated_quality,
                entry.capability.cost_per_task,
            ),
            None => (0.8, 0.5), // conservative fallback
        };

        let elapsed = start.elapsed();

        let result = TaskResultMsg {
            from: contracted_agent,
            task_id,
            quality_self_report: quality,
            payload: payload.clone(),
            elapsed,
            actual_cost: cost,
        };

        // ── Step 8: L5 record success & update trust ────────────────

        self.circuit_breaker.record_success(&contracted_agent);

        // Remove checkpoint (task completed successfully).
        self.checkpoint_store.remove(&task_id);

        // ── Step 9: L1 trust update via interaction record ──────────

        let record = atp_types::InteractionRecord {
            evaluator: self.agent_id,
            subject: contracted_agent,
            task_type,
            quality_score: quality,
            latency_ms: elapsed.as_millis() as u64,
            cost,
            timestamp: chrono::Utc::now(),
            signature: Vec::new(),
        };
        self.identity_store.add_interaction(record).await;

        // Update trust in the capability registry.
        let new_trust = self
            .identity_store
            .aggregate_trust(contracted_agent, chrono::Utc::now())
            .await;
        self.capability_registry
            .update_trust(&contracted_agent, task_type, new_trust);

        info!(
            task_id = %task_id,
            agent = %contracted_agent,
            quality = quality,
            elapsed_ms = elapsed.as_millis() as u64,
            cost = cost,
            new_trust = new_trust,
            "task execution complete"
        );

        Ok(result)
    }

    // ── Message Processing ───────────────────────────────────────────

    /// Process an incoming ATP message through the dispatcher.
    ///
    /// Delegates to the internal [`MessageDispatcher`], passing references
    /// to all layer components. Returns an optional response message that
    /// the caller is responsible for transmitting via the transport layer.
    #[instrument(skip(self, msg), fields(node = %self.agent_id))]
    pub async fn handle_message(
        &self,
        msg: AtpMessage,
    ) -> Result<Option<AtpMessage>, AtpError> {
        self.dispatcher
            .dispatch(
                msg,
                &self.identity_store,
                &self.capability_registry,
                &self.heartbeat_monitor,
                &self.circuit_breaker,
                &self.checkpoint_store,
                &self.poison_detector,
                &self.load_tracker,
            )
            .await
    }

    /// Alias for [`handle_message`](Self::handle_message) — process an
    /// incoming ATP message through the dispatcher.
    pub async fn process_message(
        &self,
        msg: AtpMessage,
    ) -> Result<Option<AtpMessage>, AtpError> {
        self.handle_message(msg).await
    }

    // ── Layer accessors ──────────────────────────────────────────────

    /// Reference to the identity store (L1).
    pub fn identity_store(&self) -> &IdentityStore {
        &self.identity_store
    }

    /// Mutable reference to the identity store (L1).
    pub fn identity_store_mut(&mut self) -> &mut IdentityStore {
        &mut self.identity_store
    }

    /// Reference to the capability registry (L2).
    pub fn capability_registry(&self) -> &CapabilityRegistry {
        &self.capability_registry
    }

    /// Mutable reference to the capability registry (L2).
    pub fn capability_registry_mut(&mut self) -> &mut CapabilityRegistry {
        &mut self.capability_registry
    }

    /// Reference to the handshake coordinator (L2).
    pub fn handshake_coordinator(&self) -> &HandshakeCoordinator {
        &self.handshake_coordinator
    }

    /// Mutable reference to the handshake coordinator (L2).
    pub fn handshake_coordinator_mut(&mut self) -> &mut HandshakeCoordinator {
        &mut self.handshake_coordinator
    }

    /// Reference to the context compressor (L3).
    pub fn context_compressor(&self) -> &ContextCompressor {
        &self.context_compressor
    }

    /// Mutable reference to the context compressor (L3).
    pub fn context_compressor_mut(&mut self) -> &mut ContextCompressor {
        &mut self.context_compressor
    }

    /// Reference to the economic router (L4).
    pub fn economic_router(&self) -> &EconomicRouter {
        &self.economic_router
    }

    /// Mutable reference to the economic router (L4).
    pub fn economic_router_mut(&mut self) -> &mut EconomicRouter {
        &mut self.economic_router
    }

    /// Reference to the heartbeat monitor (L5).
    pub fn heartbeat_monitor(&self) -> &HeartbeatMonitor {
        &self.heartbeat_monitor
    }

    /// Reference to the circuit breaker (L5).
    pub fn circuit_breaker(&self) -> &CircuitBreaker {
        &self.circuit_breaker
    }

    /// Reference to the checkpoint store (L5).
    pub fn checkpoint_store(&self) -> &CheckpointStore {
        &self.checkpoint_store
    }

    /// Reference to the poison detector (L5).
    pub fn poison_detector(&self) -> &PoisonDetector {
        &self.poison_detector
    }

    /// Reference to the load tracker (L5).
    pub fn load_tracker(&self) -> &AgentLoadTracker {
        &self.load_tracker
    }

    // ── Convenience helpers ──────────────────────────────────────────

    /// Query the current trust score for an agent on a given task type.
    pub async fn trust_score(
        &self,
        agent: AgentId,
        task_type: TaskType,
    ) -> f64 {
        let ts = self
            .identity_store
            .trust_score(agent, task_type, chrono::Utc::now())
            .await;
        ts.score
    }

    /// Generate a heartbeat message from this node.
    pub fn generate_heartbeat(&self, sequence: u64, queue_depth: u32, load_factor: f64) -> AtpMessage {
        AtpMessage::Heartbeat(atp_types::HeartbeatMsg {
            from: self.agent_id,
            sequence,
            queue_depth,
            load_factor,
        })
    }

    /// Check the health status of a remote agent via the heartbeat monitor.
    pub fn agent_health(
        &self,
        agent: &AgentId,
    ) -> Result<atp_fault::HeartbeatStatus, atp_types::FaultError> {
        self.heartbeat_monitor.status(agent)
    }

    /// Get the list of all alive agents according to the heartbeat monitor.
    pub fn alive_agents(&self) -> Vec<AgentId> {
        self.heartbeat_monitor.alive_agents()
    }

    /// Get all agents with open circuits.
    pub fn open_circuits(&self) -> Vec<AgentId> {
        self.circuit_breaker.open_circuits()
    }

    /// Record a task failure for an agent (updates circuit breaker and
    /// poison detector).
    pub fn record_task_failure(&self, task_id: uuid::Uuid, agent: AgentId) -> PoisonStatus {
        self.circuit_breaker.record_failure(&agent);
        self.poison_detector.record_failure(task_id, agent)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use atp_identity::{DidGenerator, KeyPair};

    fn make_cap(task_type: TaskType, quality: f64, latency_ms: u64, cost: f64) -> Capability {
        Capability {
            task_type,
            estimated_quality: quality,
            estimated_latency: Duration::from_millis(latency_ms),
            cost_per_task: cost,
        }
    }

    fn make_identity_with_caps(caps: Vec<Capability>) -> AgentIdentity {
        let kp = KeyPair::generate().unwrap();
        let mut identity = DidGenerator::create_identity(&kp).unwrap();
        identity.capabilities = caps;
        identity
    }

    #[tokio::test]
    async fn test_new_node() {
        let node = AtpNode::new(AtpConfig::default());
        let _ = node.id();
    }

    #[tokio::test]
    async fn test_register_agent() {
        let mut node = AtpNode::new(AtpConfig::default());

        let identity = make_identity_with_caps(vec![
            make_cap(TaskType::CodeGeneration, 0.9, 100, 0.5),
        ]);
        let agent_id = identity.id;

        node.register_agent(identity).await.unwrap();

        // Verify identity store.
        let retrieved = node.identity_store().get_identity(&agent_id).await;
        assert!(retrieved.is_ok());

        // Verify capability registry.
        let entry = node
            .capability_registry()
            .get_capability(&agent_id, TaskType::CodeGeneration);
        assert!(entry.is_some());
    }

    #[tokio::test]
    async fn test_execute_task_happy_path() {
        let mut node = AtpNode::new(AtpConfig::default());

        // Register an agent that can handle CodeGeneration.
        let identity = make_identity_with_caps(vec![
            make_cap(TaskType::CodeGeneration, 0.9, 100, 0.5),
        ]);
        let agent_id = identity.id;
        node.register_agent(identity).await.unwrap();

        // Execute a task.
        let qos = QoSConstraints {
            min_quality: 0.7,
            max_latency: Duration::from_secs(10),
            max_cost: 1.0,
            min_trust: 0.3,
        };

        let result = node
            .execute_task(
                TaskType::CodeGeneration,
                b"function parse_json()".to_vec(),
                qos,
            )
            .await;

        assert!(result.is_ok());
        let task_result = result.unwrap();
        assert_eq!(task_result.from, agent_id);
        assert!(task_result.quality_self_report >= 0.7);
        assert!(task_result.actual_cost <= 1.0);
    }

    #[tokio::test]
    async fn test_execute_task_no_capable_agents() {
        let mut node = AtpNode::new(AtpConfig::default());

        let qos = QoSConstraints::default();
        let result = node
            .execute_task(
                TaskType::Analysis,
                b"analyze this".to_vec(),
                qos,
            )
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_trust_updated_after_execution() {
        let mut node = AtpNode::new(AtpConfig::default());

        let identity = make_identity_with_caps(vec![
            make_cap(TaskType::DataProcessing, 0.85, 50, 0.3),
        ]);
        let agent_id = identity.id;
        node.register_agent(identity).await.unwrap();

        let qos = QoSConstraints {
            min_quality: 0.5,
            max_latency: Duration::from_secs(10),
            max_cost: 1.0,
            min_trust: 0.3,
        };

        node.execute_task(
            TaskType::DataProcessing,
            b"process data".to_vec(),
            qos,
        )
        .await
        .unwrap();

        // Trust should reflect the interaction.
        let trust = node.trust_score(agent_id, TaskType::DataProcessing).await;
        assert!(
            (trust - 0.85).abs() < 0.01,
            "expected trust ~0.85, got {trust}"
        );
    }

    #[tokio::test]
    async fn test_handle_message_heartbeat() {
        let node = AtpNode::new(AtpConfig::default());
        let sender = AgentId::new();

        let msg = AtpMessage::Heartbeat(atp_types::HeartbeatMsg {
            from: sender,
            sequence: 1,
            queue_depth: 10,
            load_factor: 0.2,
        });

        let result = node.handle_message(msg).await;
        assert!(result.is_ok());
        // No backpressure expected for low load.
        assert!(result.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_process_message_delegates_to_handle_message() {
        let node = AtpNode::new(AtpConfig::default());
        let sender = AgentId::new();

        let msg = AtpMessage::Heartbeat(atp_types::HeartbeatMsg {
            from: sender,
            sequence: 5,
            queue_depth: 2,
            load_factor: 0.1,
        });

        let result = node.process_message(msg).await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_register_capability() {
        let mut node = AtpNode::new(AtpConfig::default());
        let cap = make_cap(TaskType::Analysis, 0.88, 200, 0.6);

        node.register_capability(cap, 0.9);

        let entry = node
            .capability_registry()
            .get_capability(&node.id(), TaskType::Analysis);
        assert!(entry.is_some());
        let entry = entry.unwrap();
        assert!((entry.capability.estimated_quality - 0.88).abs() < f64::EPSILON);
        assert!((entry.trust_score - 0.9).abs() < f64::EPSILON);
    }

    #[tokio::test]
    async fn test_record_task_failure() {
        let node = AtpNode::new(AtpConfig::default());
        let task_id = uuid::Uuid::new_v4();
        let agent = AgentId::new();

        let status = node.record_task_failure(task_id, agent);
        assert_eq!(status, PoisonStatus::Healthy);

        // After 3 distinct agents fail the same task, it becomes poisoned.
        node.record_task_failure(task_id, AgentId::new());
        let status = node.record_task_failure(task_id, AgentId::new());
        assert_eq!(status, PoisonStatus::Poisoned);
    }

    #[tokio::test]
    async fn test_generate_heartbeat() {
        let node = AtpNode::new(AtpConfig::default());
        let msg = node.generate_heartbeat(42, 5, 0.3);
        match msg {
            AtpMessage::Heartbeat(hb) => {
                assert_eq!(hb.from, node.id());
                assert_eq!(hb.sequence, 42);
                assert_eq!(hb.queue_depth, 5);
            }
            _ => panic!("expected Heartbeat message"),
        }
    }

    #[tokio::test]
    async fn test_layer_accessors() {
        let node = AtpNode::new(AtpConfig::default());

        // Verify all accessors return valid references.
        let _ = node.identity_store();
        let _ = node.capability_registry();
        let _ = node.handshake_coordinator();
        let _ = node.context_compressor();
        let _ = node.economic_router();
        let _ = node.heartbeat_monitor();
        let _ = node.circuit_breaker();
        let _ = node.checkpoint_store();
        let _ = node.poison_detector();
        let _ = node.load_tracker();
        let _ = node.config();
    }

    #[tokio::test]
    async fn test_multiple_task_executions_update_trust() {
        let mut node = AtpNode::new(AtpConfig::default());

        let identity = make_identity_with_caps(vec![
            make_cap(TaskType::Analysis, 0.92, 80, 0.4),
        ]);
        let agent_id = identity.id;
        node.register_agent(identity).await.unwrap();

        let qos = QoSConstraints {
            min_quality: 0.5,
            max_latency: Duration::from_secs(10),
            max_cost: 1.0,
            min_trust: 0.3,
        };

        // Execute multiple tasks.
        for _ in 0..5 {
            node.execute_task(TaskType::Analysis, b"analyze".to_vec(), qos.clone())
                .await
                .unwrap();
        }

        // Trust should converge towards the agent's quality.
        let trust = node.trust_score(agent_id, TaskType::Analysis).await;
        assert!(
            (trust - 0.92).abs() < 0.02,
            "expected trust ~0.92, got {trust}"
        );

        // Verify interactions were recorded.
        let interactions = node.identity_store().get_interactions(&agent_id).await;
        assert_eq!(interactions.len(), 5);
    }
}
