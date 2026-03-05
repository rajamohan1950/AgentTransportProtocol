//! Phase 3: CONTRACT_ACCEPT and handshake state machine.
//!
//! After ranking offers the requester selects the best candidate and sends
//! a `CONTRACT_ACCEPT`.  Both parties are then bound by the agreed QoS
//! terms until the contract expires.
//!
//! The handshake progresses through a strict state machine:
//!
//! ```text
//! Idle ──► ProbeSent ──► OffersReceived ──► Contracted
//!              │               │                 
//!              └───────────────┴──────────► Failed
//! ```

use atp_types::{AgentId, ContractAcceptMsg, HandshakeError, QoSConstraints};
use chrono::{Duration as ChronoDuration, Utc};
use uuid::Uuid;

use crate::offer::RankedOffer;

/// The states of the handshake state machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HandshakeState {
    /// Initial state — no probe has been sent.
    Idle,
    /// A CAPABILITY_PROBE has been broadcast; waiting for offers.
    ProbeSent,
    /// One or more CAPABILITY_OFFERs have been received and ranked.
    OffersReceived,
    /// A CONTRACT_ACCEPT has been sent and acknowledged; handshake complete.
    Contracted,
    /// The handshake failed (timeout, no offers, or exhausted retries).
    Failed,
}

impl std::fmt::Display for HandshakeState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HandshakeState::Idle => write!(f, "Idle"),
            HandshakeState::ProbeSent => write!(f, "ProbeSent"),
            HandshakeState::OffersReceived => write!(f, "OffersReceived"),
            HandshakeState::Contracted => write!(f, "Contracted"),
            HandshakeState::Failed => write!(f, "Failed"),
        }
    }
}

/// State machine that enforces valid handshake transitions.
#[derive(Debug)]
pub struct HandshakeStateMachine {
    state: HandshakeState,
    /// The contract ID once contracted.
    contract_id: Option<Uuid>,
    /// The selected agent once contracted.
    selected_agent: Option<AgentId>,
}

impl HandshakeStateMachine {
    /// Create a new state machine in the Idle state.
    pub fn new() -> Self {
        Self {
            state: HandshakeState::Idle,
            contract_id: None,
            selected_agent: None,
        }
    }

    /// Current state.
    pub fn state(&self) -> &HandshakeState {
        &self.state
    }

    /// Contract ID (only available in Contracted state).
    pub fn contract_id(&self) -> Option<Uuid> {
        self.contract_id
    }

    /// Selected agent (only available in Contracted state).
    pub fn selected_agent(&self) -> Option<AgentId> {
        self.selected_agent
    }

    /// Transition: Idle → ProbeSent.
    pub fn on_probe_sent(&mut self) -> Result<(), HandshakeError> {
        match self.state {
            HandshakeState::Idle => {
                self.state = HandshakeState::ProbeSent;
                Ok(())
            }
            _ => Err(HandshakeError::InvalidTransition {
                from: self.state.to_string(),
                to: "ProbeSent".to_string(),
            }),
        }
    }

    /// Transition: ProbeSent → OffersReceived.
    pub fn on_offers_received(&mut self) -> Result<(), HandshakeError> {
        match self.state {
            HandshakeState::ProbeSent => {
                self.state = HandshakeState::OffersReceived;
                Ok(())
            }
            _ => Err(HandshakeError::InvalidTransition {
                from: self.state.to_string(),
                to: "OffersReceived".to_string(),
            }),
        }
    }

    /// Transition: OffersReceived → Contracted.
    pub fn on_contract_accepted(
        &mut self,
        contract_id: Uuid,
        agent: AgentId,
    ) -> Result<(), HandshakeError> {
        match self.state {
            HandshakeState::OffersReceived => {
                self.state = HandshakeState::Contracted;
                self.contract_id = Some(contract_id);
                self.selected_agent = Some(agent);
                Ok(())
            }
            _ => Err(HandshakeError::InvalidTransition {
                from: self.state.to_string(),
                to: "Contracted".to_string(),
            }),
        }
    }

    /// Transition: any non-terminal → Failed.
    pub fn on_failure(&mut self) -> Result<(), HandshakeError> {
        match self.state {
            HandshakeState::Contracted | HandshakeState::Failed => {
                Err(HandshakeError::InvalidTransition {
                    from: self.state.to_string(),
                    to: "Failed".to_string(),
                })
            }
            _ => {
                self.state = HandshakeState::Failed;
                Ok(())
            }
        }
    }

    /// Reset back to Idle for a retry attempt. Only valid from ProbeSent
    /// (timeout with no offers) or OffersReceived (all offers expired).
    pub fn reset_for_retry(&mut self) -> Result<(), HandshakeError> {
        match self.state {
            HandshakeState::ProbeSent | HandshakeState::OffersReceived => {
                self.state = HandshakeState::Idle;
                self.contract_id = None;
                self.selected_agent = None;
                Ok(())
            }
            _ => Err(HandshakeError::InvalidTransition {
                from: self.state.to_string(),
                to: "Idle".to_string(),
            }),
        }
    }

    /// Whether the state machine is in a terminal state.
    pub fn is_terminal(&self) -> bool {
        matches!(
            self.state,
            HandshakeState::Contracted | HandshakeState::Failed
        )
    }
}

impl Default for HandshakeStateMachine {
    fn default() -> Self {
        Self::new()
    }
}

/// Create a CONTRACT_ACCEPT message from the best ranked offer.
///
/// The agreed QoS is derived from the winning offer's capability values,
/// bounded by the requester's original constraints. The contract expires
/// after the given duration.
///
/// The `signature` field is left empty and must be filled by the caller.
pub fn create_contract(
    from: AgentId,
    best_offer: &RankedOffer,
    original_qos: &QoSConstraints,
    contract_duration: std::time::Duration,
) -> ContractAcceptMsg {
    let cap = &best_offer.offer.capability;

    // Agreed QoS: use the offer's actual values as the agreed terms,
    // but do not exceed the requester's original constraints.
    let agreed_qos = QoSConstraints {
        min_quality: cap.estimated_quality.min(original_qos.min_quality),
        max_latency: cap
            .estimated_latency
            .max(original_qos.max_latency),
        max_cost: cap.cost_per_task.max(original_qos.max_cost),
        min_trust: best_offer
            .offer
            .trust_score
            .min(original_qos.min_trust),
    };

    let contract_id = Uuid::new_v4();
    let now = Utc::now();
    let expires_at =
        now + ChronoDuration::from_std(contract_duration).unwrap_or(ChronoDuration::seconds(30));

    ContractAcceptMsg {
        from,
        to: best_offer.offer.from,
        agreed_qos,
        context_plan: format!("direct-{}", best_offer.offer.from),
        contract_id,
        expires_at,
        timestamp: now,
        signature: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use atp_types::{Capability, CapabilityOfferMsg, TaskType};
    use std::time::Duration;

    #[test]
    fn state_machine_happy_path() {
        let mut sm = HandshakeStateMachine::new();
        assert_eq!(*sm.state(), HandshakeState::Idle);

        sm.on_probe_sent().unwrap();
        assert_eq!(*sm.state(), HandshakeState::ProbeSent);

        sm.on_offers_received().unwrap();
        assert_eq!(*sm.state(), HandshakeState::OffersReceived);

        let cid = Uuid::new_v4();
        let agent = AgentId::new();
        sm.on_contract_accepted(cid, agent).unwrap();
        assert_eq!(*sm.state(), HandshakeState::Contracted);
        assert_eq!(sm.contract_id(), Some(cid));
        assert_eq!(sm.selected_agent(), Some(agent));
        assert!(sm.is_terminal());
    }

    #[test]
    fn state_machine_rejects_invalid_transitions() {
        let mut sm = HandshakeStateMachine::new();

        // Can't go directly to OffersReceived from Idle
        assert!(sm.on_offers_received().is_err());

        // Can't go directly to Contracted from Idle
        assert!(sm.on_contract_accepted(Uuid::new_v4(), AgentId::new()).is_err());
    }

    #[test]
    fn state_machine_failure_from_probe_sent() {
        let mut sm = HandshakeStateMachine::new();
        sm.on_probe_sent().unwrap();
        sm.on_failure().unwrap();
        assert_eq!(*sm.state(), HandshakeState::Failed);
        assert!(sm.is_terminal());
    }

    #[test]
    fn state_machine_retry_from_probe_sent() {
        let mut sm = HandshakeStateMachine::new();
        sm.on_probe_sent().unwrap();
        sm.reset_for_retry().unwrap();
        assert_eq!(*sm.state(), HandshakeState::Idle);
    }

    #[test]
    fn create_contract_generates_valid_message() {
        let offer = RankedOffer {
            score: 0.85,
            offer: CapabilityOfferMsg {
                from: AgentId::new(),
                in_reply_to: 123,
                capability: Capability {
                    task_type: TaskType::Analysis,
                    estimated_quality: 0.9,
                    estimated_latency: Duration::from_millis(100),
                    cost_per_task: 0.5,
                },
                trust_score: 0.8,
                trust_proof: Vec::new(),
                ttl: Duration::from_secs(5),
                timestamp: Utc::now(),
                signature: Vec::new(),
            },
        };

        let qos = QoSConstraints::default();
        let contract = create_contract(AgentId::new(), &offer, &qos, Duration::from_secs(60));

        assert_eq!(contract.to, offer.offer.from);
        assert!(contract.expires_at > Utc::now());
    }
}
