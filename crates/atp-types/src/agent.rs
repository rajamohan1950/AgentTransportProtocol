use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::capability::Capability;

/// Unique agent identifier wrapping a UUID v4.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AgentId(pub Uuid);

impl AgentId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for AgentId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for AgentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// W3C Decentralized Identifier bound to an ed25519 key.
/// Format: did:key:z6Mk...
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Did {
    pub method: String,
    pub identifier: String,
}

impl Did {
    pub fn to_uri(&self) -> String {
        format!("did:{}:{}", self.method, self.identifier)
    }
}

impl std::fmt::Display for Did {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_uri())
    }
}

/// An agent's public identity in the network.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentIdentity {
    pub id: AgentId,
    pub did: Did,
    pub public_key: Vec<u8>,
    pub capabilities: Vec<Capability>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}
