//! # atp-node — Composition Root
//!
//! This crate wires the five ATP protocol layers into a single, coherent
//! agent node:
//!
//! | Layer | Crate | Responsibility |
//! |-------|-------|----------------|
//! | L1 | `atp-identity` | DID, ed25519, trust scoring, Sybil resistance |
//! | L2 | `atp-handshake` | 3-phase capability negotiation |
//! | L3 | `atp-context` | Semantic Context Differentials |
//! | L4 | `atp-routing` | Economic multi-objective routing |
//! | L5 | `atp-fault` | Heartbeat, circuit breaker, checkpoint, poison |
//!
//! ## Quick Start
//!
//! ```rust,no_run
//! use atp_node::{AtpNode, AtpNodeBuilder};
//! use atp_types::{AtpConfig, QoSConstraints, TaskType};
//!
//! # async fn example() {
//! // Build a node with default configuration.
//! let mut node = AtpNodeBuilder::new()
//!     .with_config(AtpConfig::default())
//!     .build()
//!     .await
//!     .expect("failed to build node");
//!
//! // Or create directly from config.
//! let mut node = AtpNode::new(AtpConfig::default());
//!
//! // Register agents, then execute tasks through the full protocol stack.
//! let result = node.execute_task(
//!     TaskType::CodeGeneration,
//!     b"parse JSON".to_vec(),
//!     QoSConstraints::default(),
//! ).await;
//! # }
//! ```

pub mod builder;
pub mod dispatcher;
pub mod node;

// Re-export primary types for convenience.
pub use builder::AtpNodeBuilder;
pub use dispatcher::MessageDispatcher;
pub use node::AtpNode;
