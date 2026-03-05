//! Builder pattern for configuring [`AtpNode`](crate::node::AtpNode).
//!
//! The builder allows callers to inject custom implementations of each
//! protocol layer, or fall back to sensible defaults derived from the
//! [`AtpConfig`].
//!
//! # Example
//!
//! ```rust,no_run
//! use atp_node::AtpNodeBuilder;
//! use atp_types::AtpConfig;
//!
//! # async fn example() {
//! let node = AtpNodeBuilder::new()
//!     .with_config(AtpConfig::default())
//!     .build()
//!     .await
//!     .expect("failed to build node");
//! # }
//! ```



use atp_context::ContextCompressor;
use atp_fault::{
    AgentLoadTracker, CheckpointStore, CircuitBreaker, HeartbeatMonitor, PoisonDetector,
};
use atp_handshake::{CapabilityRegistry, HandshakeCoordinator};
use atp_identity::IdentityStore;
use atp_routing::{AgentGraph, EconomicRouter};
use atp_types::{AgentId, AtpConfig, AtpError};

use crate::node::AtpNode;

/// Builder for constructing a fully wired [`AtpNode`].
///
/// Each layer component can be explicitly provided; any component that is
/// not set will be created with defaults from the [`AtpConfig`].
pub struct AtpNodeBuilder {
    config: AtpConfig,
    agent_id: Option<AgentId>,

    // Layer 1: Identity
    identity_store: Option<IdentityStore>,

    // Layer 2: Handshake
    capability_registry: Option<CapabilityRegistry>,
    // HandshakeCoordinator is created from agent_id + config, but we
    // allow an optional override.
    handshake_coordinator: Option<HandshakeCoordinator>,

    // Layer 3: Context
    context_compressor: Option<ContextCompressor>,

    // Layer 4: Routing
    economic_router: Option<EconomicRouter>,

    // Layer 5: Fault Tolerance
    heartbeat_monitor: Option<HeartbeatMonitor>,
    circuit_breaker: Option<CircuitBreaker>,
    checkpoint_store: Option<CheckpointStore>,
    poison_detector: Option<PoisonDetector>,
    load_tracker: Option<AgentLoadTracker>,
}

impl AtpNodeBuilder {
    /// Create a new builder with default configuration.
    pub fn new() -> Self {
        Self {
            config: AtpConfig::default(),
            agent_id: None,
            identity_store: None,
            capability_registry: None,
            handshake_coordinator: None,
            context_compressor: None,
            economic_router: None,
            heartbeat_monitor: None,
            circuit_breaker: None,
            checkpoint_store: None,
            poison_detector: None,
            load_tracker: None,
        }
    }

    // ── Configuration ────────────────────────────────────────────────

    /// Set the full configuration. Defaults for any unset layer components
    /// will be derived from this config.
    pub fn with_config(mut self, config: AtpConfig) -> Self {
        self.config = config;
        self
    }

    /// Set a specific agent ID for this node.
    ///
    /// If not set, a new random `AgentId` is generated.
    pub fn with_agent_id(mut self, id: AgentId) -> Self {
        self.agent_id = Some(id);
        self
    }

    // ── Layer 1: Identity ────────────────────────────────────────────

    /// Inject a custom identity store.
    pub fn with_identity_store(mut self, store: IdentityStore) -> Self {
        self.identity_store = Some(store);
        self
    }

    // ── Layer 2: Handshake ───────────────────────────────────────────

    /// Inject a custom capability registry.
    pub fn with_capability_registry(mut self, registry: CapabilityRegistry) -> Self {
        self.capability_registry = Some(registry);
        self
    }

    /// Inject a custom handshake coordinator.
    pub fn with_handshake_coordinator(mut self, coordinator: HandshakeCoordinator) -> Self {
        self.handshake_coordinator = Some(coordinator);
        self
    }

    // ── Layer 3: Context ─────────────────────────────────────────────

    /// Inject a custom context compressor.
    pub fn with_context_compressor(mut self, compressor: ContextCompressor) -> Self {
        self.context_compressor = Some(compressor);
        self
    }

    // ── Layer 4: Routing ─────────────────────────────────────────────

    /// Inject a custom economic router.
    pub fn with_economic_router(mut self, router: EconomicRouter) -> Self {
        self.economic_router = Some(router);
        self
    }

    // ── Layer 5: Fault Tolerance ─────────────────────────────────────

    /// Inject a custom heartbeat monitor.
    pub fn with_heartbeat_monitor(mut self, monitor: HeartbeatMonitor) -> Self {
        self.heartbeat_monitor = Some(monitor);
        self
    }

    /// Inject a custom circuit breaker.
    pub fn with_circuit_breaker(mut self, breaker: CircuitBreaker) -> Self {
        self.circuit_breaker = Some(breaker);
        self
    }

    /// Inject a custom checkpoint store.
    pub fn with_checkpoint_store(mut self, store: CheckpointStore) -> Self {
        self.checkpoint_store = Some(store);
        self
    }

    /// Inject a custom poison detector.
    pub fn with_poison_detector(mut self, detector: PoisonDetector) -> Self {
        self.poison_detector = Some(detector);
        self
    }

    /// Inject a custom load tracker.
    pub fn with_load_tracker(mut self, tracker: AgentLoadTracker) -> Self {
        self.load_tracker = Some(tracker);
        self
    }

    // ── Build ────────────────────────────────────────────────────────

    /// Consume the builder and produce a fully wired [`AtpNode`].
    ///
    /// Any layer component that was not explicitly provided is created
    /// with sensible defaults derived from the stored [`AtpConfig`].
    pub async fn build(self) -> Result<AtpNode, AtpError> {
        let agent_id = self.agent_id.unwrap_or_default();

        // -- Layer 1: Identity --
        let identity_store = self.identity_store.unwrap_or_else(|| {
            let trust_engine = atp_identity::TrustEngine::new(
                self.config.identity.trust_decay_rate,
                0.5, // default prior
            );
            let sybil_guard = atp_identity::SybilGuard::new(
                self.config.identity.sybil_dampening,
                5,
                atp_identity::TrustEngine::new(
                    self.config.identity.trust_decay_rate,
                    0.5,
                ),
            );
            IdentityStore::with_engines(trust_engine, sybil_guard)
        });

        // -- Layer 2: Handshake --
        let capability_registry = self
            .capability_registry
            .unwrap_or_default();

        let handshake_coordinator = self.handshake_coordinator.unwrap_or_else(|| {
            HandshakeCoordinator::new(agent_id, self.config.handshake.clone())
        });

        // -- Layer 3: Context --
        let context_compressor = self.context_compressor.unwrap_or_else(|| {
            let msc_config = atp_context::MscConfig {
                relevance_threshold: self.config.context.relevance_threshold,
                max_chunks: 10,
                chunk_size: 512,
                dimensions: self.config.context.embedding_dimensions,
            };
            ContextCompressor::with_config(msc_config)
        });

        // -- Layer 4: Routing --
        let economic_router = self.economic_router.unwrap_or_else(|| {
            EconomicRouter::with_config(
                AgentGraph::new(),
                atp_routing::CostModel::default(),
                self.config.routing.clone(),
            )
        });

        // -- Layer 5: Fault Tolerance --
        let heartbeat_monitor = self
            .heartbeat_monitor
            .unwrap_or_else(|| HeartbeatMonitor::new(self.config.fault.clone()));

        let circuit_breaker = self.circuit_breaker.unwrap_or_else(|| {
            let cooldown = self.config.fault.heartbeat_interval
                * self.config.fault.heartbeat_timeout_multiplier
                * 10;
            CircuitBreaker::new(self.config.fault.clone(), cooldown)
        });

        let checkpoint_store = self
            .checkpoint_store
            .unwrap_or_default();

        let poison_detector = self
            .poison_detector
            .unwrap_or_else(|| PoisonDetector::new(self.config.fault.clone()));

        let load_tracker = self
            .load_tracker
            .unwrap_or_else(|| AgentLoadTracker::new(self.config.fault.clone()));

        Ok(AtpNode::from_components(
            agent_id,
            self.config,
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
        ))
    }
}

impl Default for AtpNodeBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;
    use super::*;

    #[tokio::test]
    async fn test_build_default() {
        let node = AtpNodeBuilder::new().build().await.unwrap();
        // Should have a valid agent ID.
        let _ = node.id();
    }

    #[tokio::test]
    async fn test_build_with_custom_id() {
        let custom_id = AgentId::new();
        let node = AtpNodeBuilder::new()
            .with_agent_id(custom_id)
            .build()
            .await
            .unwrap();
        assert_eq!(node.id(), custom_id);
    }

    #[tokio::test]
    async fn test_build_with_custom_config() {
        let mut config = AtpConfig::default();
        config.context.relevance_threshold = 0.5;
        config.fault.circuit_breaker_threshold = 5;

        let node = AtpNodeBuilder::new()
            .with_config(config)
            .build()
            .await
            .unwrap();

        let _ = node.id();
    }

    #[tokio::test]
    async fn test_build_with_injected_store() {
        let store = IdentityStore::new();
        let node = AtpNodeBuilder::new()
            .with_identity_store(store)
            .build()
            .await
            .unwrap();
        let _ = node.id();
    }

    #[tokio::test]
    async fn test_build_with_injected_registry() {
        let mut registry = CapabilityRegistry::new();
        let agent = AgentId::new();
        registry.register(
            agent,
            atp_types::Capability {
                task_type: atp_types::TaskType::Analysis,
                estimated_quality: 0.9,
                estimated_latency: Duration::from_millis(100),
                cost_per_task: 0.5,
            },
            0.85,
        );

        let node = AtpNodeBuilder::new()
            .with_agent_id(agent)
            .with_capability_registry(registry)
            .build()
            .await
            .unwrap();

        assert_eq!(node.id(), agent);
    }
}
