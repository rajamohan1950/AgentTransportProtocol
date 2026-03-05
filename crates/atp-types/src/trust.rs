use crate::{AgentId, TaskType};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A single interaction record for trust computation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InteractionRecord {
    pub evaluator: AgentId,
    pub subject: AgentId,
    pub task_type: TaskType,
    pub quality_score: f64,
    pub latency_ms: u64,
    pub cost: f64,
    pub timestamp: DateTime<Utc>,
    pub signature: Vec<u8>,
}

/// Per-capability trust score for an agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustScore {
    pub agent: AgentId,
    pub task_type: TaskType,
    pub score: f64,
    pub sample_count: u32,
    pub last_updated: DateTime<Utc>,
}

/// Trust vector: map from TaskType -> trust score.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TrustVector {
    pub scores: HashMap<TaskType, f64>,
}

impl TrustVector {
    pub fn get(&self, task_type: TaskType) -> f64 {
        self.scores.get(&task_type).copied().unwrap_or(0.5)
    }

    pub fn set(&mut self, task_type: TaskType, score: f64) {
        self.scores.insert(task_type, score.clamp(0.0, 1.0));
    }
}
