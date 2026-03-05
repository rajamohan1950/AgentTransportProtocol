//! Agent graph — adjacency list with weighted edges.
//!
//! The graph G = (V, E) represents the agent network topology where:
//! - Vertices (V) are agents with their capabilities
//! - Edges (E) are directed connections with transfer latency weights
//!
//! Agents are indexed by dense integer indices for O(1) lookup.
//! An `AgentId -> index` map provides the bridge to external identifiers.

use std::collections::HashMap;
use std::time::Duration;

use atp_types::{AgentId, Capability, RoutingError, TaskType};

/// A node (agent) in the routing graph.
#[derive(Debug, Clone)]
pub struct AgentNode {
    /// External agent identifier.
    pub id: AgentId,
    /// All capabilities this agent advertises.
    pub capabilities: Vec<Capability>,
    /// Whether this agent is currently available for routing.
    pub available: bool,
    /// Trust score (0.0 to 1.0) — used for filtering, not as an edge weight.
    pub trust_score: f64,
}

impl AgentNode {
    /// Find the capability matching a given task type.
    pub fn capability_for(&self, task_type: TaskType) -> Option<&Capability> {
        self.capabilities.iter().find(|c| c.task_type == task_type)
    }
}

/// A directed edge in the routing graph.
#[derive(Debug, Clone)]
pub struct Edge {
    /// Source agent index.
    pub from: usize,
    /// Destination agent index.
    pub to: usize,
    /// Network transfer latency for this link.
    pub transfer_latency: Duration,
}

/// Agent routing graph with adjacency list representation.
///
/// Supports O(1) node lookup by index, O(degree) edge traversal,
/// and O(k) capability matching where k = number of agents.
#[derive(Debug, Clone)]
pub struct AgentGraph {
    /// Agent nodes indexed by dense integer.
    nodes: Vec<AgentNode>,
    /// Adjacency list: adjacency[i] = list of edges from node i.
    adjacency: Vec<Vec<Edge>>,
    /// Map from AgentId to internal index for O(1) lookup.
    id_to_index: HashMap<AgentId, usize>,
}

impl AgentGraph {
    /// Create an empty graph.
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            adjacency: Vec::new(),
            id_to_index: HashMap::new(),
        }
    }

    /// Create a graph with pre-allocated capacity for `n` agents.
    pub fn with_capacity(n: usize) -> Self {
        Self {
            nodes: Vec::with_capacity(n),
            adjacency: Vec::with_capacity(n),
            id_to_index: HashMap::with_capacity(n),
        }
    }

    /// Add an agent to the graph. Returns its internal index.
    ///
    /// If the agent already exists, updates its capabilities and returns
    /// the existing index.
    pub fn add_agent(
        &mut self,
        id: AgentId,
        capabilities: Vec<Capability>,
        trust_score: f64,
    ) -> usize {
        if let Some(&idx) = self.id_to_index.get(&id) {
            // Update existing agent.
            self.nodes[idx].capabilities = capabilities;
            self.nodes[idx].trust_score = trust_score;
            self.nodes[idx].available = true;
            return idx;
        }

        let idx = self.nodes.len();
        self.nodes.push(AgentNode {
            id,
            capabilities,
            available: true,
            trust_score,
        });
        self.adjacency.push(Vec::new());
        self.id_to_index.insert(id, idx);
        idx
    }

    /// Add a directed edge from one agent to another with a given transfer latency.
    pub fn add_edge(&mut self, from: AgentId, to: AgentId, transfer_latency: Duration) {
        if let (Some(&from_idx), Some(&to_idx)) =
            (self.id_to_index.get(&from), self.id_to_index.get(&to))
        {
            // Avoid duplicate edges.
            let exists = self.adjacency[from_idx]
                .iter()
                .any(|e| e.to == to_idx);
            if !exists {
                self.adjacency[from_idx].push(Edge {
                    from: from_idx,
                    to: to_idx,
                    transfer_latency,
                });
            }
        }
    }

    /// Add bidirectional edge between two agents.
    pub fn add_bidi_edge(&mut self, a: AgentId, b: AgentId, transfer_latency: Duration) {
        self.add_edge(a, b, transfer_latency);
        self.add_edge(b, a, transfer_latency);
    }

    /// Remove an agent from routing consideration (soft delete).
    pub fn set_unavailable(&mut self, id: AgentId) {
        if let Some(&idx) = self.id_to_index.get(&id) {
            self.nodes[idx].available = false;
        }
    }

    /// Re-enable an agent for routing.
    pub fn set_available(&mut self, id: AgentId) {
        if let Some(&idx) = self.id_to_index.get(&id) {
            self.nodes[idx].available = true;
        }
    }

    /// Number of agents (nodes) in the graph.
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Number of directed edges in the graph.
    pub fn edge_count(&self) -> usize {
        self.adjacency.iter().map(|adj| adj.len()).sum()
    }

    /// Get a node by its internal index.
    pub fn node(&self, idx: usize) -> Option<&AgentNode> {
        self.nodes.get(idx)
    }

    /// Get a node by AgentId.
    pub fn node_by_id(&self, id: AgentId) -> Option<&AgentNode> {
        self.id_to_index.get(&id).and_then(|&idx| self.nodes.get(idx))
    }

    /// Get the internal index for an AgentId.
    pub fn index_of(&self, id: AgentId) -> Option<usize> {
        self.id_to_index.get(&id).copied()
    }

    /// Get the AgentId for an internal index.
    pub fn agent_id(&self, idx: usize) -> Option<AgentId> {
        self.nodes.get(idx).map(|n| n.id)
    }

    /// Get all outgoing edges from a node.
    pub fn edges_from(&self, idx: usize) -> &[Edge] {
        self.adjacency.get(idx).map(|v| v.as_slice()).unwrap_or(&[])
    }

    /// Get the transfer latency between two nodes, if an edge exists.
    pub fn transfer_latency(&self, from: usize, to: usize) -> Option<Duration> {
        self.adjacency
            .get(from)?
            .iter()
            .find(|e| e.to == to)
            .map(|e| e.transfer_latency)
    }

    /// Find all agents capable of handling a given task type.
    ///
    /// Returns indices of available agents with the requested capability
    /// and trust above the given threshold. This is the "capability matching"
    /// step that produces the subgraph for Bellman-Ford.
    pub fn capable_agents(
        &self,
        task_type: TaskType,
        min_trust: f64,
    ) -> Vec<usize> {
        self.nodes
            .iter()
            .enumerate()
            .filter(|(_, node)| {
                node.available
                    && node.trust_score >= min_trust
                    && node.capability_for(task_type).is_some()
            })
            .map(|(idx, _)| idx)
            .collect()
    }

    /// Build the complete subgraph of capable agents.
    /// Returns (agent_indices, edges_between_them).
    pub fn capability_subgraph(
        &self,
        task_type: TaskType,
        min_trust: f64,
    ) -> (Vec<usize>, Vec<Edge>) {
        let agents = self.capable_agents(task_type, min_trust);
        let agent_set: std::collections::HashSet<usize> =
            agents.iter().copied().collect();

        let mut edges = Vec::new();
        for &idx in &agents {
            for edge in &self.adjacency[idx] {
                if agent_set.contains(&edge.to) {
                    edges.push(edge.clone());
                }
            }
        }

        (agents, edges)
    }

    /// Connect all capable agents to each other with a default latency.
    /// Useful for building fully connected subgraph for small agent sets.
    pub fn fully_connect(&mut self, latency: Duration) {
        let n = self.nodes.len();
        for i in 0..n {
            for j in 0..n {
                if i != j {
                    let exists = self.adjacency[i].iter().any(|e| e.to == j);
                    if !exists {
                        self.adjacency[i].push(Edge {
                            from: i,
                            to: j,
                            transfer_latency: latency,
                        });
                    }
                }
            }
        }
    }

    /// All node indices.
    pub fn all_indices(&self) -> impl Iterator<Item = usize> {
        0..self.nodes.len()
    }

    /// All nodes.
    pub fn nodes(&self) -> &[AgentNode] {
        &self.nodes
    }

    /// Validate that the graph is non-empty.
    pub fn validate(&self) -> Result<(), RoutingError> {
        if self.nodes.is_empty() {
            return Err(RoutingError::EmptyGraph);
        }
        Ok(())
    }
}

impl Default for AgentGraph {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_cap(task: TaskType, quality: f64, latency_ms: u64, cost: f64) -> Capability {
        Capability {
            task_type: task,
            estimated_quality: quality,
            estimated_latency: Duration::from_millis(latency_ms),
            cost_per_task: cost,
        }
    }

    #[test]
    fn test_add_and_lookup() {
        let mut g = AgentGraph::new();
        let a1 = AgentId::new();
        let a2 = AgentId::new();
        let idx1 = g.add_agent(a1, vec![make_cap(TaskType::Analysis, 0.9, 100, 0.5)], 0.8);
        let idx2 = g.add_agent(a2, vec![make_cap(TaskType::Analysis, 0.7, 200, 0.3)], 0.6);
        assert_eq!(g.node_count(), 2);
        assert_eq!(g.index_of(a1), Some(idx1));
        assert_eq!(g.agent_id(idx2), Some(a2));
    }

    #[test]
    fn test_edges() {
        let mut g = AgentGraph::new();
        let a1 = AgentId::new();
        let a2 = AgentId::new();
        g.add_agent(a1, vec![], 1.0);
        g.add_agent(a2, vec![], 1.0);
        g.add_bidi_edge(a1, a2, Duration::from_millis(5));
        assert_eq!(g.edge_count(), 2);
        let idx1 = g.index_of(a1).unwrap();
        let idx2 = g.index_of(a2).unwrap();
        assert_eq!(
            g.transfer_latency(idx1, idx2),
            Some(Duration::from_millis(5))
        );
    }

    #[test]
    fn test_capable_agents() {
        let mut g = AgentGraph::new();
        let a1 = AgentId::new();
        let a2 = AgentId::new();
        let a3 = AgentId::new();
        g.add_agent(a1, vec![make_cap(TaskType::Analysis, 0.9, 100, 0.5)], 0.8);
        g.add_agent(a2, vec![make_cap(TaskType::CodeGeneration, 0.7, 200, 0.3)], 0.6);
        g.add_agent(a3, vec![make_cap(TaskType::Analysis, 0.5, 50, 0.1)], 0.3);

        let capable = g.capable_agents(TaskType::Analysis, 0.5);
        assert_eq!(capable.len(), 1); // a1 only, a3 trust too low
    }

    #[test]
    fn test_availability() {
        let mut g = AgentGraph::new();
        let a1 = AgentId::new();
        g.add_agent(a1, vec![make_cap(TaskType::Analysis, 0.9, 100, 0.5)], 0.8);
        assert_eq!(g.capable_agents(TaskType::Analysis, 0.5).len(), 1);

        g.set_unavailable(a1);
        assert_eq!(g.capable_agents(TaskType::Analysis, 0.5).len(), 0);

        g.set_available(a1);
        assert_eq!(g.capable_agents(TaskType::Analysis, 0.5).len(), 1);
    }
}
