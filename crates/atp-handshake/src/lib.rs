//! # atp-handshake — Layer 2: Capability Handshake
//!
//! Implements the three-phase capability negotiation protocol modeled after
//! TCP's SYN / SYN-ACK / ACK handshake:
//!
//! 1. **CAPABILITY_PROBE** — A requester broadcasts its task requirements and
//!    QoS constraints to discover capable agents.
//! 2. **CAPABILITY_OFFER** — Candidate agents respond with scored bids
//!    (quality, latency, cost, trust).
//! 3. **CONTRACT_ACCEPT** — The requester selects the best offer; both parties
//!    are bound by agreed QoS terms.
//!
//! The [`HandshakeCoordinator`] orchestrates the full flow with configurable
//! timeouts (default 500 ms per phase), automatic QoS relaxation (10 % per
//! retry, up to 3 retries), and a strict state machine
//! (`Idle → ProbeSent → OffersReceived → Contracted | Failed`).

pub mod registry;
pub mod probe;
pub mod offer;
pub mod contract;
pub mod negotiation;

pub use registry::CapabilityRegistry;
pub use probe::{create_probe, process_probe};
pub use offer::{create_offer, rank_offers, OfferRanker, RankedOffer};
pub use contract::{create_contract, HandshakeState, HandshakeStateMachine};
pub use negotiation::{HandshakeCoordinator, HandshakeOutcome};
