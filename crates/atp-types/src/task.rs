use crate::{AgentId, ContextDiff, TaskType};
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Status of a task in the pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskStatus {
    Pending,
    Assigned,
    InProgress,
    Completed,
    Failed,
    Poisoned,
}

/// A task to be executed by an agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: uuid::Uuid,
    pub task_type: TaskType,
    pub payload: Vec<u8>,
    pub context: Option<ContextDiff>,
    pub status: TaskStatus,
    pub assigned_to: Option<AgentId>,
    pub submitted_at: chrono::DateTime<chrono::Utc>,
}

/// Result of task execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskResult {
    pub task_id: uuid::Uuid,
    pub from: AgentId,
    pub quality_self_report: f64,
    pub payload: Vec<u8>,
    pub elapsed: Duration,
    pub actual_cost: f64,
}
