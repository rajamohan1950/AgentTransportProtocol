use crate::agent::{AgentArchetypes, SimulatedAgent};
use atp_types::*;
use rand::Rng;
use std::collections::HashMap;
use std::time::Duration;

/// Network topology for simulation.
#[derive(Debug, Clone)]
pub enum NetworkTopology {
    /// Every agent connected to every other.
    FullyConnected,
    /// Random edges with given connectivity probability.
    Random { connectivity: f64 },
    /// Small-world network (Watts-Strogatz).
    SmallWorld { k: usize, beta: f64 },
}

/// Edge latency between two agents.
#[derive(Debug, Clone)]
pub struct EdgeLatency {
    pub from: AgentId,
    pub to: AgentId,
    pub latency: Duration,
}

/// A simulated multi-agent network.
#[derive(Debug)]
pub struct SimulatedNetwork {
    pub agents: Vec<SimulatedAgent>,
    pub topology: NetworkTopology,
    pub edges: Vec<EdgeLatency>,
    agent_index: HashMap<AgentId, usize>,
}

impl SimulatedNetwork {
    /// Create the benchmark network: 50 agents across archetypes, 4 task types.
    pub fn benchmark_network(seed: u64) -> Self {
        use rand::SeedableRng;
        let mut rng = rand::rngs::StdRng::seed_from_u64(seed);

        let all_types = TaskType::all();
        let mut agents = Vec::with_capacity(50);

        // 10 budget agents (all types)
        for _ in 0..10 {
            agents.push(AgentArchetypes::budget(AgentId::new(), all_types));
        }

        // 15 standard agents (all types)
        for _ in 0..15 {
            agents.push(AgentArchetypes::standard(AgentId::new(), all_types));
        }

        // 8 premium agents (all types)
        for _ in 0..8 {
            agents.push(AgentArchetypes::premium(AgentId::new(), all_types));
        }

        // 12 specialists (3 per task type)
        for &tt in all_types {
            for _ in 0..3 {
                agents.push(AgentArchetypes::specialist(AgentId::new(), tt));
            }
        }

        // 5 unreliable agents (all types)
        for _ in 0..5 {
            agents.push(AgentArchetypes::unreliable(AgentId::new(), all_types));
        }

        let topology = NetworkTopology::SmallWorld { k: 6, beta: 0.3 };
        let mut network = Self {
            agent_index: HashMap::new(),
            agents,
            topology,
            edges: Vec::new(),
        };

        // Build index
        for (i, agent) in network.agents.iter().enumerate() {
            network.agent_index.insert(agent.id, i);
        }

        // Generate edges based on topology
        network.generate_edges(&mut rng);
        network
    }

    /// Create a custom network.
    pub fn new(agents: Vec<SimulatedAgent>, topology: NetworkTopology, seed: u64) -> Self {
        use rand::SeedableRng;
        let mut rng = rand::rngs::StdRng::seed_from_u64(seed);

        let mut network = Self {
            agent_index: HashMap::new(),
            agents,
            topology,
            edges: Vec::new(),
        };

        for (i, agent) in network.agents.iter().enumerate() {
            network.agent_index.insert(agent.id, i);
        }

        network.generate_edges(&mut rng);
        network
    }

    fn generate_edges<R: Rng>(&mut self, rng: &mut R) {
        let n = self.agents.len();
        self.edges.clear();

        match &self.topology {
            NetworkTopology::FullyConnected => {
                for i in 0..n {
                    for j in 0..n {
                        if i != j {
                            let latency_ms = rng.gen_range(1..50);
                            self.edges.push(EdgeLatency {
                                from: self.agents[i].id,
                                to: self.agents[j].id,
                                latency: Duration::from_millis(latency_ms),
                            });
                        }
                    }
                }
            }
            NetworkTopology::Random { connectivity } => {
                for i in 0..n {
                    for j in 0..n {
                        if i != j && rng.gen::<f64>() < *connectivity {
                            let latency_ms = rng.gen_range(5..100);
                            self.edges.push(EdgeLatency {
                                from: self.agents[i].id,
                                to: self.agents[j].id,
                                latency: Duration::from_millis(latency_ms),
                            });
                        }
                    }
                }
            }
            NetworkTopology::SmallWorld { k, beta } => {
                // Watts-Strogatz: start with ring lattice, rewire with probability beta
                let k = *k;
                let beta = *beta;

                // Ring lattice: connect each node to k/2 neighbors on each side
                let half_k = k / 2;
                for i in 0..n {
                    for offset in 1..=half_k {
                        let j = (i + offset) % n;
                        let latency_ms = rng.gen_range(5..50);
                        self.edges.push(EdgeLatency {
                            from: self.agents[i].id,
                            to: self.agents[j].id,
                            latency: Duration::from_millis(latency_ms),
                        });
                        self.edges.push(EdgeLatency {
                            from: self.agents[j].id,
                            to: self.agents[i].id,
                            latency: Duration::from_millis(latency_ms),
                        });
                    }
                }

                // Rewire with probability beta
                let original_edges = self.edges.clone();
                self.edges.clear();
                for edge in original_edges {
                    if rng.gen::<f64>() < beta {
                        // Rewire to random node
                        let new_target_idx = rng.gen_range(0..n);
                        let new_target = self.agents[new_target_idx].id;
                        if new_target != edge.from {
                            self.edges.push(EdgeLatency {
                                from: edge.from,
                                to: new_target,
                                latency: Duration::from_millis(rng.gen_range(5..100)),
                            });
                        }
                    } else {
                        self.edges.push(edge);
                    }
                }
            }
        }
    }

    pub fn get_agent(&self, id: &AgentId) -> Option<&SimulatedAgent> {
        self.agent_index.get(id).map(|&i| &self.agents[i])
    }

    pub fn get_agent_mut(&mut self, id: &AgentId) -> Option<&mut SimulatedAgent> {
        self.agent_index.get(id).copied().map(|i| &mut self.agents[i])
    }

    /// Find agents capable of handling a task type.
    pub fn capable_agents(&self, task_type: TaskType) -> Vec<&SimulatedAgent> {
        self.agents
            .iter()
            .filter(|a| a.has_capability(task_type))
            .collect()
    }

    /// Get edge latency between two agents.
    pub fn edge_latency(&self, from: &AgentId, to: &AgentId) -> Duration {
        self.edges
            .iter()
            .find(|e| e.from == *from && e.to == *to)
            .map(|e| e.latency)
            .unwrap_or(Duration::from_millis(50)) // default if no direct edge
    }

    pub fn agent_count(&self) -> usize {
        self.agents.len()
    }
}
