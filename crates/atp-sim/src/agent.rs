use atp_types::*;
use rand::Rng;
use rand_distr::{Distribution, LogNormal, Normal};
use std::time::Duration;

/// A simulated agent with configurable performance distributions.
#[derive(Debug, Clone)]
pub struct SimulatedAgent {
    pub id: AgentId,
    pub identity: AgentIdentity,
    pub capabilities: Vec<SimulatedCapability>,
    pub failure_rate: f64,
    pub queue_capacity: usize,
    pub current_queue_depth: u32,
}

/// A capability with stochastic quality and latency distributions.
#[derive(Debug, Clone)]
pub struct SimulatedCapability {
    pub task_type: TaskType,
    /// Quality follows a truncated normal distribution.
    pub quality_mean: f64,
    pub quality_std: f64,
    /// Latency follows a log-normal distribution (ms).
    pub latency_mean_ms: f64,
    pub latency_std_ms: f64,
    /// Fixed cost per task in USD.
    pub cost: f64,
}

impl SimulatedCapability {
    pub fn to_capability(&self) -> Capability {
        Capability {
            task_type: self.task_type,
            estimated_quality: self.quality_mean,
            estimated_latency: Duration::from_millis(self.latency_mean_ms as u64),
            cost_per_task: self.cost,
        }
    }
}

/// Result of simulating a task execution.
#[derive(Debug, Clone)]
pub struct SimulatedTaskResult {
    pub agent_id: AgentId,
    pub task_type: TaskType,
    pub quality: f64,
    pub latency: Duration,
    pub cost: f64,
    pub success: bool,
}

impl SimulatedAgent {
    pub fn new(id: AgentId, capabilities: Vec<SimulatedCapability>, failure_rate: f64) -> Self {
        let did = Did {
            method: "key".to_string(),
            identifier: format!("z6Mk{}", id.0.as_simple()),
        };
        let identity = AgentIdentity {
            id,
            did,
            public_key: vec![0u8; 32],
            capabilities: capabilities.iter().map(|c| c.to_capability()).collect(),
            created_at: chrono::Utc::now(),
        };
        Self {
            id,
            identity,
            capabilities,
            failure_rate,
            queue_capacity: 100,
            current_queue_depth: 0,
        }
    }

    /// Simulate executing a task. Returns quality/latency/cost with stochastic noise.
    pub fn execute_task<R: Rng>(&self, task_type: TaskType, rng: &mut R) -> SimulatedTaskResult {
        // Check for failure
        if rng.gen::<f64>() < self.failure_rate {
            return SimulatedTaskResult {
                agent_id: self.id,
                task_type,
                quality: 0.0,
                latency: Duration::from_millis(100),
                cost: 0.0,
                success: false,
            };
        }

        // Find matching capability
        let default_cap = SimulatedCapability {
            task_type,
            quality_mean: 0.5,
            quality_std: 0.15,
            latency_mean_ms: 5000.0,
            latency_std_ms: 1.0,
            cost: 0.1,
        };
        let cap = self
            .capabilities
            .iter()
            .find(|c| c.task_type == task_type)
            .unwrap_or(&default_cap);

        // Sample quality from truncated normal
        let quality_dist = Normal::new(cap.quality_mean, cap.quality_std).unwrap();
        let quality = quality_dist.sample(rng).clamp(0.0, 1.0);

        // Sample latency from log-normal
        let ln_mean = cap.latency_mean_ms.ln();
        let latency_dist = LogNormal::new(ln_mean, cap.latency_std_ms.max(0.01)).unwrap();
        let latency_ms = latency_dist.sample(rng).max(1.0);

        SimulatedTaskResult {
            agent_id: self.id,
            task_type,
            quality,
            latency: Duration::from_millis(latency_ms as u64),
            cost: cap.cost,
            success: true,
        }
    }

    pub fn has_capability(&self, task_type: TaskType) -> bool {
        self.capabilities.iter().any(|c| c.task_type == task_type)
    }

    pub fn get_capability(&self, task_type: TaskType) -> Option<&SimulatedCapability> {
        self.capabilities.iter().find(|c| c.task_type == task_type)
    }
}

/// Agent archetypes for building realistic networks.
pub struct AgentArchetypes;

impl AgentArchetypes {
    /// Cheap, fast, lower quality agent.
    pub fn budget(id: AgentId, task_types: &[TaskType]) -> SimulatedAgent {
        let caps = task_types
            .iter()
            .map(|&tt| SimulatedCapability {
                task_type: tt,
                quality_mean: 0.65,
                quality_std: 0.1,
                latency_mean_ms: 500.0,
                latency_std_ms: 0.3,
                cost: 0.01,
            })
            .collect();
        SimulatedAgent::new(id, caps, 0.02)
    }

    /// Mid-range balanced agent.
    pub fn standard(id: AgentId, task_types: &[TaskType]) -> SimulatedAgent {
        let caps = task_types
            .iter()
            .map(|&tt| SimulatedCapability {
                task_type: tt,
                quality_mean: 0.82,
                quality_std: 0.08,
                latency_mean_ms: 2000.0,
                latency_std_ms: 0.4,
                cost: 0.05,
            })
            .collect();
        SimulatedAgent::new(id, caps, 0.03)
    }

    /// Expensive, slow, high quality agent.
    pub fn premium(id: AgentId, task_types: &[TaskType]) -> SimulatedAgent {
        let caps = task_types
            .iter()
            .map(|&tt| SimulatedCapability {
                task_type: tt,
                quality_mean: 0.95,
                quality_std: 0.03,
                latency_mean_ms: 5000.0,
                latency_std_ms: 0.5,
                cost: 0.15,
            })
            .collect();
        SimulatedAgent::new(id, caps, 0.01)
    }

    /// Specialist: high quality in one area, unavailable for others.
    pub fn specialist(id: AgentId, speciality: TaskType) -> SimulatedAgent {
        let caps = vec![SimulatedCapability {
            task_type: speciality,
            quality_mean: 0.93,
            quality_std: 0.04,
            latency_mean_ms: 1500.0,
            latency_std_ms: 0.3,
            cost: 0.08,
        }];
        SimulatedAgent::new(id, caps, 0.015)
    }

    /// Unreliable agent: decent quality but high failure rate.
    pub fn unreliable(id: AgentId, task_types: &[TaskType]) -> SimulatedAgent {
        let caps = task_types
            .iter()
            .map(|&tt| SimulatedCapability {
                task_type: tt,
                quality_mean: 0.80,
                quality_std: 0.12,
                latency_mean_ms: 1000.0,
                latency_std_ms: 0.8,
                cost: 0.03,
            })
            .collect();
        SimulatedAgent::new(id, caps, 0.15)
    }
}
