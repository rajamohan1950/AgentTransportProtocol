use atp_types::*;
use rand::Rng;

/// Generates tasks for benchmarking.
pub struct TaskGenerator {
    task_types: Vec<TaskType>,
    context_size: usize,
}

impl TaskGenerator {
    pub fn new() -> Self {
        Self {
            task_types: TaskType::all().to_vec(),
            context_size: 50_000, // 50K tokens typical enterprise task
        }
    }

    pub fn with_context_size(mut self, size: usize) -> Self {
        self.context_size = size;
        self
    }

    /// Generate a batch of tasks with uniform distribution across categories.
    pub fn generate<R: Rng>(&self, count: usize, rng: &mut R) -> Vec<SimTask> {
        (0..count)
            .map(|_| {
                let task_type = self.task_types[rng.gen_range(0..self.task_types.len())];
                let payload_size = rng.gen_range(100..1000);
                let payload: Vec<u8> = (0..payload_size).map(|_| rng.gen()).collect();

                SimTask {
                    id: uuid::Uuid::new_v4(),
                    task_type,
                    payload,
                    full_context_size: self.context_size,
                    qos: QoSConstraints {
                        min_quality: 0.7 + rng.gen::<f64>() * 0.2, // 0.7-0.9
                        max_latency: std::time::Duration::from_millis(
                            rng.gen_range(2000..10000),
                        ),
                        max_cost: 0.05 + rng.gen::<f64>() * 0.15, // $0.05-$0.20
                        min_trust: 0.5 + rng.gen::<f64>() * 0.3,  // 0.5-0.8
                    },
                }
            })
            .collect()
    }
}

impl Default for TaskGenerator {
    fn default() -> Self {
        Self::new()
    }
}

/// A task for simulation.
#[derive(Debug, Clone)]
pub struct SimTask {
    pub id: uuid::Uuid,
    pub task_type: TaskType,
    pub payload: Vec<u8>,
    pub full_context_size: usize,
    pub qos: QoSConstraints,
}
