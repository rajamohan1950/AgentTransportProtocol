//! ATP Transport Layer -- gRPC server and client for the Agent Transport Protocol.
//!
//! This crate provides:
//!
//! - **`codec`**: Bidirectional conversions between `atp_types` domain types and
//!   `atp_proto` generated protobuf types, using `From`/`TryFrom` traits.
//!
//! - **`server`**: A tonic gRPC server ([`AtpServer`](server::AtpServer)) that
//!   implements all 10 ATP service RPCs. Protocol layers are plugged in via
//!   handler traits, with sensible defaults for development.
//!
//! - **`client`**: A typed gRPC client ([`AtpClient`](client::AtpClient)) that
//!   wraps the generated tonic client and provides domain-typed methods.

pub mod codec;
pub mod server;
pub mod client;

// Re-export key types for convenience
pub use client::AtpClient;
pub use codec::CodecError;
pub use server::{AtpServer, AtpServerBuilder};
