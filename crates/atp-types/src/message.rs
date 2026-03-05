use crate::*;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// The 10 ATP message types.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AtpMessage {
    CapabilityProbe(CapabilityProbeMsg),
    CapabilityOffer(CapabilityOfferMsg),
    ContractAccept(ContractAcceptMsg),
    ContextRequest(ContextRequestMsg),
    TaskSubmit(TaskSubmitMsg),
    TaskResult(TaskResultMsg),
    InteractionProof(InteractionProofMsg),
    Heartbeat(HeartbeatMsg),
    Backpressure(BackpressureMsg),
    CircuitBreak(CircuitBreakMsg),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityProbeMsg {
    pub from: AgentId,
    pub task_type: TaskType,
    pub qos: QoSConstraints,
    pub context_embedding: Option<ContextEmbedding>,
    pub nonce: u64,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub signature: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityOfferMsg {
    pub from: AgentId,
    pub in_reply_to: u64,
    pub capability: Capability,
    pub trust_score: f64,
    pub trust_proof: Vec<u8>,
    pub ttl: Duration,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub signature: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractAcceptMsg {
    pub from: AgentId,
    pub to: AgentId,
    pub agreed_qos: QoSConstraints,
    pub context_plan: String,
    pub contract_id: uuid::Uuid,
    pub expires_at: chrono::DateTime<chrono::Utc>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub signature: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextRequestMsg {
    pub from: AgentId,
    pub to: AgentId,
    pub task_id: uuid::Uuid,
    pub current_confidence: f64,
    pub requested_chunk_indices: Vec<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskSubmitMsg {
    pub from: AgentId,
    pub to: AgentId,
    pub task_id: uuid::Uuid,
    pub task_type: TaskType,
    pub payload: Vec<u8>,
    pub context: ContextDiff,
    pub contract_id: uuid::Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskResultMsg {
    pub from: AgentId,
    pub task_id: uuid::Uuid,
    pub quality_self_report: f64,
    pub payload: Vec<u8>,
    pub elapsed: Duration,
    pub actual_cost: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InteractionProofMsg {
    pub evaluator: AgentId,
    pub subject: AgentId,
    pub task_id: uuid::Uuid,
    pub task_type: TaskType,
    pub quality_score: f64,
    pub latency_ms: u64,
    pub cost: f64,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub signature: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatMsg {
    pub from: AgentId,
    pub sequence: u64,
    pub queue_depth: u32,
    pub load_factor: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackpressureMsg {
    pub from: AgentId,
    pub queue_depth: u32,
    pub recommended_rate: f64,
    pub estimated_drain_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CircuitState {
    Closed,
    Open,
    HalfOpen,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitBreakMsg {
    pub from: AgentId,
    pub target: AgentId,
    pub state: CircuitState,
    pub failure_count: u32,
}
