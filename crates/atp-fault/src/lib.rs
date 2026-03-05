//! # atp-fault -- Layer 5: Fault Tolerance
//!
//! Provides heartbeat monitoring, circuit breaking, checkpoint/failover,
//! poison-task detection, and backpressure signaling for the Agent Transport
//! Protocol.

pub mod backpressure;
pub mod checkpoint;
pub mod circuit_breaker;
pub mod heartbeat;
pub mod poison;

pub use backpressure::{AgentLoadTracker, BackpressureSignal};
pub use checkpoint::{CheckpointData, CheckpointStore, FailoverDecision};
pub use circuit_breaker::CircuitBreaker;
pub use heartbeat::{HeartbeatMonitor, HeartbeatStatus};
pub use poison::{PoisonDetector, PoisonStatus};
