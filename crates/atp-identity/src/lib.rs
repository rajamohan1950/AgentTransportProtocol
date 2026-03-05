//! # atp-identity — Layer 1: Identity & Trust
//!
//! This crate provides Ed25519 key management, W3C DID generation using the
//! `did:key` method, time-decayed trust scoring, Sybil-resistance via
//! transitive dampening, and an in-memory identity/interaction store.

pub mod did;
pub mod keypair;
pub mod sybil;
pub mod store;
pub mod trust;

pub use did::DidGenerator;
pub use keypair::KeyPair;
pub use store::IdentityStore;
pub use sybil::SybilGuard;
pub use trust::TrustEngine;
