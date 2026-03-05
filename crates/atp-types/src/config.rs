use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Top-level ATP configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[derive(Default)]
pub struct AtpConfig {
    pub identity: IdentityConfig,
    pub handshake: HandshakeConfig,
    pub context: ContextConfig,
    pub routing: RoutingConfig,
    pub fault: FaultConfig,
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdentityConfig {
    /// Time decay rate lambda (per day). Default: 0.01.
    pub trust_decay_rate: f64,
    /// Sybil dampening factor alpha. Default: 0.5.
    pub sybil_dampening: f64,
    /// Minimum interactions for trust to be considered valid.
    pub min_interactions: u32,
}

impl Default for IdentityConfig {
    fn default() -> Self {
        Self {
            trust_decay_rate: 0.01,
            sybil_dampening: 0.5,
            min_interactions: 3,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandshakeConfig {
    /// Probe timeout before retry/relaxation.
    pub probe_timeout: Duration,
    /// QoS relaxation factor per retry.
    pub relaxation_factor: f64,
    /// Maximum retry attempts.
    pub max_retries: u32,
}

impl Default for HandshakeConfig {
    fn default() -> Self {
        Self {
            probe_timeout: Duration::from_millis(500),
            relaxation_factor: 0.1,
            max_retries: 3,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextConfig {
    /// Cosine similarity threshold for MSC extraction.
    pub relevance_threshold: f64,
    /// Confidence threshold for adaptive context requests.
    pub confidence_threshold: f64,
    /// Default embedding dimensions.
    pub embedding_dimensions: usize,
}

impl Default for ContextConfig {
    fn default() -> Self {
        Self {
            relevance_threshold: 0.3,
            confidence_threshold: 0.7,
            embedding_dimensions: 768,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingConfig {
    /// Route recomputation interval.
    pub recompute_interval: Duration,
    /// Route TTL.
    pub route_ttl: Duration,
    /// Maximum routes to return from find_routes.
    pub max_routes: usize,
}

impl Default for RoutingConfig {
    fn default() -> Self {
        Self {
            recompute_interval: Duration::from_secs(60),
            route_ttl: Duration::from_secs(120),
            max_routes: 10,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FaultConfig {
    /// Heartbeat interval.
    pub heartbeat_interval: Duration,
    /// Heartbeat timeout multiplier (failure = multiplier * interval).
    pub heartbeat_timeout_multiplier: u32,
    /// Consecutive failures to trip circuit breaker.
    pub circuit_breaker_threshold: u32,
    /// Time window for poison task detection.
    pub poison_detection_window: Duration,
    /// Agent failures within window to mark task poisoned.
    pub poison_agent_threshold: u32,
    /// Queue depth threshold for backpressure.
    pub backpressure_threshold: u32,
}

impl Default for FaultConfig {
    fn default() -> Self {
        Self {
            heartbeat_interval: Duration::from_secs(1),
            heartbeat_timeout_multiplier: 3,
            circuit_breaker_threshold: 3,
            poison_detection_window: Duration::from_secs(60),
            poison_agent_threshold: 3,
            backpressure_threshold: 100,
        }
    }
}
