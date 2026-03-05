//! Checkpoint storage and failover logic.
//!
//! An in-memory store keeps the latest serialised state snapshot for each
//! task so that a failed agent's work can be resumed by a replacement.

use std::collections::HashMap;
use std::sync::RwLock;

use atp_types::{AgentId, FaultError};
use chrono::{DateTime, Utc};
use tracing::{debug, info, warn};
use uuid::Uuid;

/// A snapshot of a task's intermediate state.
#[derive(Debug, Clone)]
pub struct CheckpointData {
    /// The task this checkpoint belongs to.
    pub task_id: Uuid,
    /// Opaque serialised state (owned by the executing agent's logic).
    pub state_bytes: Vec<u8>,
    /// The agent that produced this checkpoint.
    pub agent_id: AgentId,
    /// Wall-clock time the checkpoint was taken.
    pub timestamp: DateTime<Utc>,
    /// Monotonically increasing version for conflict detection.
    pub version: u64,
}

/// The outcome of a failover decision.
#[derive(Debug, Clone)]
pub struct FailoverDecision {
    /// The task to be reassigned.
    pub task_id: Uuid,
    /// The checkpoint to resume from (if any).
    pub checkpoint: Option<CheckpointData>,
    /// The agent that previously held the task.
    pub failed_agent: AgentId,
    /// The newly assigned agent.
    pub new_agent: AgentId,
    /// When the decision was made.
    pub decided_at: DateTime<Utc>,
}

/// In-memory checkpoint store keyed by task ID.
///
/// Each task retains only its latest checkpoint (newest version wins).
pub struct CheckpointStore {
    /// task_id → latest checkpoint
    store: RwLock<HashMap<Uuid, CheckpointData>>,
    /// task_id → full version history (kept for diagnostics; bounded
    /// externally if desired)
    history: RwLock<HashMap<Uuid, Vec<CheckpointData>>>,
}

impl CheckpointStore {
    pub fn new() -> Self {
        Self {
            store: RwLock::new(HashMap::new()),
            history: RwLock::new(HashMap::new()),
        }
    }

    // ── writes ───────────────────────────────────────────────────────

    /// Save (or update) a checkpoint for the given task.
    ///
    /// Only the latest version is kept in the hot store; older versions
    /// are appended to the history log.
    pub fn save(&self, data: CheckpointData) {
        let task_id = data.task_id;
        let version = data.version;

        // Update history first.
        {
            let mut hist = self.history.write().expect("checkpoint history lock poisoned");
            hist.entry(task_id).or_default().push(data.clone());
        }

        // Update hot store (only if this is a newer version).
        {
            let mut store = self.store.write().expect("checkpoint store lock poisoned");
            let should_update = store
                .get(&task_id)
                .map(|existing| version > existing.version)
                .unwrap_or(true);

            if should_update {
                debug!(task_id = %task_id, version, "checkpoint saved");
                store.insert(task_id, data);
            }
        }
    }

    /// Convenience: create and save a checkpoint in one call.
    pub fn create_checkpoint(
        &self,
        task_id: Uuid,
        state_bytes: Vec<u8>,
        agent_id: AgentId,
        version: u64,
    ) -> CheckpointData {
        let cp = CheckpointData {
            task_id,
            state_bytes,
            agent_id,
            timestamp: Utc::now(),
            version,
        };
        self.save(cp.clone());
        cp
    }

    // ── reads ────────────────────────────────────────────────────────

    /// Retrieve the latest checkpoint for a task.
    pub fn get(&self, task_id: &Uuid) -> Option<CheckpointData> {
        let store = self.store.read().expect("checkpoint store lock poisoned");
        store.get(task_id).cloned()
    }

    /// Retrieve the full version history for a task.
    pub fn get_history(&self, task_id: &Uuid) -> Vec<CheckpointData> {
        let hist = self.history.read().expect("checkpoint history lock poisoned");
        hist.get(task_id).cloned().unwrap_or_default()
    }

    /// True if a checkpoint exists for the given task.
    pub fn has_checkpoint(&self, task_id: &Uuid) -> bool {
        let store = self.store.read().expect("checkpoint store lock poisoned");
        store.contains_key(task_id)
    }

    /// Number of tasks with checkpoints.
    pub fn count(&self) -> usize {
        let store = self.store.read().expect("checkpoint store lock poisoned");
        store.len()
    }

    /// Remove the checkpoint (and history) for a completed task to free memory.
    pub fn remove(&self, task_id: &Uuid) -> Option<CheckpointData> {
        let removed = {
            let mut store = self.store.write().expect("checkpoint store lock poisoned");
            store.remove(task_id)
        };
        {
            let mut hist = self.history.write().expect("checkpoint history lock poisoned");
            hist.remove(task_id);
        }
        removed
    }

    // ── failover ─────────────────────────────────────────────────────

    /// Build a failover decision for a task whose agent has failed.
    ///
    /// If a checkpoint exists the decision will include it so the new agent
    /// can resume from that point.  Returns `Err` only if the `new_agent`
    /// is the same as the `failed_agent` (pointless failover).
    pub fn failover(
        &self,
        task_id: Uuid,
        failed_agent: AgentId,
        new_agent: AgentId,
    ) -> Result<FailoverDecision, FaultError> {
        if failed_agent == new_agent {
            return Err(FaultError::CheckpointFailed(
                "new agent is the same as the failed agent".into(),
            ));
        }

        let checkpoint = self.get(&task_id);
        if checkpoint.is_some() {
            info!(
                task_id = %task_id,
                failed = %failed_agent,
                new = %new_agent,
                "failover with checkpoint"
            );
        } else {
            warn!(
                task_id = %task_id,
                failed = %failed_agent,
                new = %new_agent,
                "failover without checkpoint — task will restart from scratch"
            );
        }

        Ok(FailoverDecision {
            task_id,
            checkpoint,
            failed_agent,
            new_agent,
            decided_at: Utc::now(),
        })
    }
}

impl Default for CheckpointStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cp(task_id: Uuid, agent: AgentId, version: u64, data: &[u8]) -> CheckpointData {
        CheckpointData {
            task_id,
            state_bytes: data.to_vec(),
            agent_id: agent,
            timestamp: Utc::now(),
            version,
        }
    }

    #[test]
    fn test_save_and_retrieve() {
        let store = CheckpointStore::new();
        let tid = Uuid::new_v4();
        let agent = AgentId::new();

        store.save(cp(tid, agent, 1, b"state-v1"));
        let got = store.get(&tid).unwrap();
        assert_eq!(got.version, 1);
        assert_eq!(got.state_bytes, b"state-v1");
    }

    #[test]
    fn test_newer_version_overwrites() {
        let store = CheckpointStore::new();
        let tid = Uuid::new_v4();
        let agent = AgentId::new();

        store.save(cp(tid, agent, 1, b"v1"));
        store.save(cp(tid, agent, 2, b"v2"));

        let got = store.get(&tid).unwrap();
        assert_eq!(got.version, 2);
        assert_eq!(got.state_bytes, b"v2");
    }

    #[test]
    fn test_older_version_does_not_overwrite() {
        let store = CheckpointStore::new();
        let tid = Uuid::new_v4();
        let agent = AgentId::new();

        store.save(cp(tid, agent, 5, b"v5"));
        store.save(cp(tid, agent, 3, b"v3"));

        let got = store.get(&tid).unwrap();
        assert_eq!(got.version, 5);
    }

    #[test]
    fn test_history_preserved() {
        let store = CheckpointStore::new();
        let tid = Uuid::new_v4();
        let agent = AgentId::new();

        store.save(cp(tid, agent, 1, b"v1"));
        store.save(cp(tid, agent, 2, b"v2"));
        store.save(cp(tid, agent, 3, b"v3"));

        let hist = store.get_history(&tid);
        assert_eq!(hist.len(), 3);
    }

    #[test]
    fn test_remove() {
        let store = CheckpointStore::new();
        let tid = Uuid::new_v4();
        let agent = AgentId::new();

        store.save(cp(tid, agent, 1, b"data"));
        assert!(store.has_checkpoint(&tid));

        let removed = store.remove(&tid);
        assert!(removed.is_some());
        assert!(!store.has_checkpoint(&tid));
        assert!(store.get_history(&tid).is_empty());
    }

    #[test]
    fn test_create_checkpoint_convenience() {
        let store = CheckpointStore::new();
        let tid = Uuid::new_v4();
        let agent = AgentId::new();

        let created = store.create_checkpoint(tid, b"hello".to_vec(), agent, 1);
        assert_eq!(created.task_id, tid);
        assert_eq!(store.get(&tid).unwrap().state_bytes, b"hello");
    }

    #[test]
    fn test_failover_with_checkpoint() {
        let store = CheckpointStore::new();
        let tid = Uuid::new_v4();
        let failed = AgentId::new();
        let replacement = AgentId::new();

        store.save(cp(tid, failed, 1, b"partial-state"));

        let decision = store.failover(tid, failed, replacement).unwrap();
        assert!(decision.checkpoint.is_some());
        assert_eq!(decision.failed_agent, failed);
        assert_eq!(decision.new_agent, replacement);
    }

    #[test]
    fn test_failover_without_checkpoint() {
        let store = CheckpointStore::new();
        let tid = Uuid::new_v4();
        let failed = AgentId::new();
        let replacement = AgentId::new();

        let decision = store.failover(tid, failed, replacement).unwrap();
        assert!(decision.checkpoint.is_none());
    }

    #[test]
    fn test_failover_same_agent_rejected() {
        let store = CheckpointStore::new();
        let tid = Uuid::new_v4();
        let agent = AgentId::new();

        let result = store.failover(tid, agent, agent);
        assert!(result.is_err());
    }

    #[test]
    fn test_count() {
        let store = CheckpointStore::new();
        assert_eq!(store.count(), 0);

        let a = AgentId::new();
        store.save(cp(Uuid::new_v4(), a, 1, b"a"));
        store.save(cp(Uuid::new_v4(), a, 1, b"b"));
        assert_eq!(store.count(), 2);
    }
}
