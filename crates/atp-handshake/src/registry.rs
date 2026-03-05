//! In-memory capability registry.
//!
//! The [`CapabilityRegistry`] stores agent capabilities indexed by
//! [`TaskType`], enabling O(1) lookup of candidate agents when a
//! `CAPABILITY_PROBE` arrives.

use atp_types::{AgentId, Capability, QoSConstraints, TaskType};
use std::collections::HashMap;

/// An entry in the registry associating an agent with a specific capability.
#[derive(Debug, Clone)]
pub struct RegistryEntry {
    pub agent_id: AgentId,
    pub capability: Capability,
    pub trust_score: f64,
}

/// In-memory store of agent capabilities, indexed by task type for fast
/// lookup during the probe phase.
#[derive(Debug, Clone)]
pub struct CapabilityRegistry {
    /// Primary index: task_type → list of (agent, capability, trust).
    by_task_type: HashMap<TaskType, Vec<RegistryEntry>>,
    /// Secondary index: agent_id → list of capabilities.
    by_agent: HashMap<AgentId, Vec<Capability>>,
}

impl CapabilityRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self {
            by_task_type: HashMap::new(),
            by_agent: HashMap::new(),
        }
    }

    /// Register a capability for an agent with a given trust score.
    ///
    /// If the same agent re-registers for the same task type, the old
    /// entry is replaced.
    pub fn register(
        &mut self,
        agent_id: AgentId,
        capability: Capability,
        trust_score: f64,
    ) {
        let task_type = capability.task_type;

        // Remove any existing entry for this agent + task type.
        if let Some(entries) = self.by_task_type.get_mut(&task_type) {
            entries.retain(|e| e.agent_id != agent_id);
        }

        // Insert the new entry.
        self.by_task_type
            .entry(task_type)
            .or_default()
            .push(RegistryEntry {
                agent_id,
                capability: capability.clone(),
                trust_score,
            });

        // Update the per-agent secondary index.
        let agent_caps = self.by_agent.entry(agent_id).or_default();
        agent_caps.retain(|c| c.task_type != task_type);
        agent_caps.push(capability);
    }

    /// Remove all capabilities for a given agent.
    pub fn unregister(&mut self, agent_id: &AgentId) {
        for entries in self.by_task_type.values_mut() {
            entries.retain(|e| &e.agent_id != agent_id);
        }
        self.by_agent.remove(agent_id);
    }

    /// Remove a specific capability for an agent.
    pub fn unregister_capability(&mut self, agent_id: &AgentId, task_type: TaskType) {
        if let Some(entries) = self.by_task_type.get_mut(&task_type) {
            entries.retain(|e| &e.agent_id != agent_id);
        }
        if let Some(caps) = self.by_agent.get_mut(agent_id) {
            caps.retain(|c| c.task_type != task_type);
            if caps.is_empty() {
                self.by_agent.remove(agent_id);
            }
        }
    }

    /// Find all agents whose capabilities satisfy the given QoS constraints
    /// for a specific task type. Returns entries sorted by trust score
    /// (descending).
    pub fn find_capable(
        &self,
        task_type: TaskType,
        qos: &QoSConstraints,
    ) -> Vec<&RegistryEntry> {
        let mut results: Vec<&RegistryEntry> = self
            .by_task_type
            .get(&task_type)
            .map(|entries| {
                entries
                    .iter()
                    .filter(|e| Self::meets_constraints(e, qos))
                    .collect()
            })
            .unwrap_or_default();

        // Sort by trust score descending for deterministic ordering.
        results.sort_by(|a, b| {
            b.trust_score
                .partial_cmp(&a.trust_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        results
    }

    /// Find all registered capabilities for a given agent.
    pub fn get_agent_capabilities(&self, agent_id: &AgentId) -> Vec<&Capability> {
        self.by_agent
            .get(agent_id)
            .map(|caps| caps.iter().collect())
            .unwrap_or_default()
    }

    /// Get a specific capability for an agent and task type.
    pub fn get_capability(
        &self,
        agent_id: &AgentId,
        task_type: TaskType,
    ) -> Option<&RegistryEntry> {
        self.by_task_type
            .get(&task_type)
            .and_then(|entries| entries.iter().find(|e| &e.agent_id == agent_id))
    }

    /// Update the trust score for an agent in a specific task type.
    pub fn update_trust(
        &mut self,
        agent_id: &AgentId,
        task_type: TaskType,
        new_trust: f64,
    ) {
        if let Some(entries) = self.by_task_type.get_mut(&task_type) {
            for entry in entries.iter_mut() {
                if &entry.agent_id == agent_id {
                    entry.trust_score = new_trust.clamp(0.0, 1.0);
                }
            }
        }
    }

    /// Return the total number of registered (agent, capability) pairs.
    pub fn len(&self) -> usize {
        self.by_task_type.values().map(|v| v.len()).sum()
    }

    /// Whether the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Return the number of distinct registered agents.
    pub fn agent_count(&self) -> usize {
        self.by_agent.len()
    }

    /// Check whether a single entry satisfies QoS constraints.
    fn meets_constraints(entry: &RegistryEntry, qos: &QoSConstraints) -> bool {
        let cap = &entry.capability;
        cap.estimated_quality >= qos.min_quality
            && cap.estimated_latency <= qos.max_latency
            && cap.cost_per_task <= qos.max_cost
            && entry.trust_score >= qos.min_trust
    }
}

impl Default for CapabilityRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn make_cap(task_type: TaskType, quality: f64, latency_ms: u64, cost: f64) -> Capability {
        Capability {
            task_type,
            estimated_quality: quality,
            estimated_latency: Duration::from_millis(latency_ms),
            cost_per_task: cost,
        }
    }

    #[test]
    fn register_and_find() {
        let mut reg = CapabilityRegistry::new();
        let a1 = AgentId::new();
        let a2 = AgentId::new();

        reg.register(a1, make_cap(TaskType::Analysis, 0.9, 100, 0.5), 0.8);
        reg.register(a2, make_cap(TaskType::Analysis, 0.7, 200, 0.3), 0.6);

        let qos = QoSConstraints {
            min_quality: 0.6,
            max_latency: Duration::from_millis(300),
            max_cost: 1.0,
            min_trust: 0.5,
        };

        let results = reg.find_capable(TaskType::Analysis, &qos);
        assert_eq!(results.len(), 2);
        // Sorted by trust descending: a1 first
        assert_eq!(results[0].agent_id, a1);
        assert_eq!(results[1].agent_id, a2);
    }

    #[test]
    fn qos_filter_excludes_poor_agents() {
        let mut reg = CapabilityRegistry::new();
        let a1 = AgentId::new();

        reg.register(a1, make_cap(TaskType::Analysis, 0.5, 100, 0.5), 0.8);

        let qos = QoSConstraints {
            min_quality: 0.7,
            max_latency: Duration::from_millis(300),
            max_cost: 1.0,
            min_trust: 0.5,
        };

        assert!(reg.find_capable(TaskType::Analysis, &qos).is_empty());
    }

    #[test]
    fn unregister_removes_agent() {
        let mut reg = CapabilityRegistry::new();
        let a1 = AgentId::new();
        reg.register(a1, make_cap(TaskType::Analysis, 0.9, 100, 0.5), 0.8);
        assert_eq!(reg.len(), 1);

        reg.unregister(&a1);
        assert_eq!(reg.len(), 0);
        assert_eq!(reg.agent_count(), 0);
    }

    #[test]
    fn re_register_replaces_old_entry() {
        let mut reg = CapabilityRegistry::new();
        let a1 = AgentId::new();
        reg.register(a1, make_cap(TaskType::Analysis, 0.7, 100, 0.5), 0.5);
        reg.register(a1, make_cap(TaskType::Analysis, 0.9, 50, 0.3), 0.9);

        assert_eq!(reg.len(), 1);
        let entry = reg.get_capability(&a1, TaskType::Analysis).unwrap();
        assert!((entry.capability.estimated_quality - 0.9).abs() < f64::EPSILON);
        assert!((entry.trust_score - 0.9).abs() < f64::EPSILON);
    }

    #[test]
    fn update_trust_score() {
        let mut reg = CapabilityRegistry::new();
        let a1 = AgentId::new();
        reg.register(a1, make_cap(TaskType::Analysis, 0.9, 100, 0.5), 0.5);

        reg.update_trust(&a1, TaskType::Analysis, 0.95);
        let entry = reg.get_capability(&a1, TaskType::Analysis).unwrap();
        assert!((entry.trust_score - 0.95).abs() < f64::EPSILON);
    }
}
