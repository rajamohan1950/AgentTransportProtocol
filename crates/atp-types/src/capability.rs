use serde::{Deserialize, Serialize};
use std::time::Duration;

/// The four benchmark task categories from the HLD.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TaskType {
    CodeGeneration,
    Analysis,
    CreativeWriting,
    DataProcessing,
}

impl TaskType {
    /// Task complexity weight gamma(task_type) for trust scoring.
    /// Complex tasks contribute more to trust computation.
    pub fn complexity_weight(&self) -> f64 {
        match self {
            TaskType::CodeGeneration => 1.5,
            TaskType::Analysis => 1.2,
            TaskType::CreativeWriting => 1.0,
            TaskType::DataProcessing => 0.8,
        }
    }

    pub fn all() -> &'static [TaskType] {
        &[
            TaskType::CodeGeneration,
            TaskType::Analysis,
            TaskType::CreativeWriting,
            TaskType::DataProcessing,
        ]
    }
}

impl std::fmt::Display for TaskType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TaskType::CodeGeneration => write!(f, "code_generation"),
            TaskType::Analysis => write!(f, "analysis"),
            TaskType::CreativeWriting => write!(f, "creative_writing"),
            TaskType::DataProcessing => write!(f, "data_processing"),
        }
    }
}

/// A capability an agent advertises.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Capability {
    pub task_type: TaskType,
    pub estimated_quality: f64,
    pub estimated_latency: Duration,
    pub cost_per_task: f64,
}

/// Quality-of-Service constraints from a task requester.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QoSConstraints {
    pub min_quality: f64,
    pub max_latency: Duration,
    pub max_cost: f64,
    pub min_trust: f64,
}

impl QoSConstraints {
    /// Relax constraints by a factor (e.g., 0.1 = 10% relaxation).
    pub fn relax(&self, factor: f64) -> Self {
        Self {
            min_quality: (self.min_quality * (1.0 - factor)).max(0.0),
            max_latency: Duration::from_secs_f64(self.max_latency.as_secs_f64() * (1.0 + factor)),
            max_cost: self.max_cost * (1.0 + factor),
            min_trust: (self.min_trust * (1.0 - factor)).max(0.0),
        }
    }
}

impl Default for QoSConstraints {
    fn default() -> Self {
        Self {
            min_quality: 0.7,
            max_latency: Duration::from_secs(10),
            max_cost: 1.0,
            min_trust: 0.5,
        }
    }
}
