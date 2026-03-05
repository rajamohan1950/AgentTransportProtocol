//! Poison task detection.
//!
//! A task is declared **poisoned** when >= `poison_agent_threshold`
//! (default 3) *distinct* agents have failed it within a sliding
//! `poison_detection_window` (default 60 s).  Poisoned tasks are never
//! retried and should be surfaced to the requester for manual inspection.

use std::collections::{HashMap, HashSet};
use std::sync::RwLock;

use atp_types::{AgentId, FaultConfig, FaultError};
use chrono::{DateTime, Utc};
use tracing::{info, warn};
use uuid::Uuid;

/// Whether a task is healthy, under observation, or confirmed poisoned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PoisonStatus {
    /// No poison signal — retries are allowed.
    Healthy,
    /// Multiple agents have failed but the threshold is not yet met.
    Suspected,
    /// The task has been marked poisoned; do not retry.
    Poisoned,
}

/// A single failure record.
#[derive(Debug, Clone)]
struct FailureRecord {
    agent_id: AgentId,
    timestamp: DateTime<Utc>,
}

/// Poison task tracker.
///
/// Thread-safe via interior `RwLock`.
pub struct PoisonDetector {
    config: FaultConfig,
    /// task_id → list of failure records (within the sliding window).
    failures: RwLock<HashMap<Uuid, Vec<FailureRecord>>>,
    /// Tasks that have been conclusively marked as poisoned.
    poisoned: RwLock<HashSet<Uuid>>,
}

impl PoisonDetector {
    pub fn new(config: FaultConfig) -> Self {
        Self {
            config,
            failures: RwLock::new(HashMap::new()),
            poisoned: RwLock::new(HashSet::new()),
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(FaultConfig::default())
    }

    // ── recording ────────────────────────────────────────────────────

    /// Record a task failure by a specific agent.
    ///
    /// Automatically prunes stale records outside the detection window and
    /// evaluates the poison condition.  Returns the updated
    /// [`PoisonStatus`].
    pub fn record_failure(&self, task_id: Uuid, agent_id: AgentId) -> PoisonStatus {
        // If already poisoned, short-circuit.
        {
            let poisoned = self.poisoned.read().expect("poison set lock poisoned");
            if poisoned.contains(&task_id) {
                return PoisonStatus::Poisoned;
            }
        }

        let now = Utc::now();
        let window = chrono::Duration::from_std(self.config.poison_detection_window)
            .unwrap_or_else(|_| chrono::Duration::seconds(60));

        let mut failures = self.failures.write().expect("poison failures lock poisoned");
        let records = failures.entry(task_id).or_default();

        // Append the new failure.
        records.push(FailureRecord {
            agent_id,
            timestamp: now,
        });

        // Prune records outside the window.
        let cutoff = now - window;
        records.retain(|r| r.timestamp >= cutoff);

        // Count distinct agents in the window.
        let distinct_agents: HashSet<AgentId> = records.iter().map(|r| r.agent_id).collect();
        let n = distinct_agents.len() as u32;

        if n >= self.config.poison_agent_threshold {
            drop(failures); // release write lock before acquiring another
            let mut poisoned = self.poisoned.write().expect("poison set lock poisoned");
            poisoned.insert(task_id);
            warn!(
                task_id = %task_id,
                distinct_agents = n,
                "task marked POISONED"
            );
            PoisonStatus::Poisoned
        } else if n >= 2 {
            PoisonStatus::Suspected
        } else {
            PoisonStatus::Healthy
        }
    }

    // ── queries ──────────────────────────────────────────────────────

    /// Check the poison status of a task without recording anything.
    pub fn status(&self, task_id: &Uuid) -> PoisonStatus {
        {
            let poisoned = self.poisoned.read().expect("poison set lock poisoned");
            if poisoned.contains(task_id) {
                return PoisonStatus::Poisoned;
            }
        }

        let failures = self.failures.read().expect("poison failures lock poisoned");
        match failures.get(task_id) {
            None => PoisonStatus::Healthy,
            Some(records) => {
                let now = Utc::now();
                let window = chrono::Duration::from_std(self.config.poison_detection_window)
                    .unwrap_or_else(|_| chrono::Duration::seconds(60));
                let cutoff = now - window;

                let distinct: HashSet<AgentId> = records
                    .iter()
                    .filter(|r| r.timestamp >= cutoff)
                    .map(|r| r.agent_id)
                    .collect();

                if distinct.len() as u32 >= self.config.poison_agent_threshold {
                    PoisonStatus::Poisoned
                } else if distinct.len() >= 2 {
                    PoisonStatus::Suspected
                } else {
                    PoisonStatus::Healthy
                }
            }
        }
    }

    /// Whether a task is poisoned (convenience shorthand).
    pub fn is_poisoned(&self, task_id: &Uuid) -> bool {
        let poisoned = self.poisoned.read().expect("poison set lock poisoned");
        poisoned.contains(task_id)
    }

    /// Should the system retry this task?
    ///
    /// Returns `Ok(())` when retries are allowed, or
    /// `Err(FaultError::PoisonTask)` when the task is poisoned.
    pub fn allow_retry(&self, task_id: &Uuid) -> Result<(), FaultError> {
        if self.is_poisoned(task_id) {
            Err(FaultError::PoisonTask(task_id.to_string()))
        } else {
            Ok(())
        }
    }

    /// Return the set of all poisoned task IDs.
    pub fn poisoned_tasks(&self) -> HashSet<Uuid> {
        let poisoned = self.poisoned.read().expect("poison set lock poisoned");
        poisoned.clone()
    }

    /// Return the distinct agents that have failed a given task within the
    /// current detection window.
    pub fn failed_agents_for(&self, task_id: &Uuid) -> Vec<AgentId> {
        let failures = self.failures.read().expect("poison failures lock poisoned");
        match failures.get(task_id) {
            None => Vec::new(),
            Some(records) => {
                let now = Utc::now();
                let window = chrono::Duration::from_std(self.config.poison_detection_window)
                    .unwrap_or_else(|_| chrono::Duration::seconds(60));
                let cutoff = now - window;

                let mut seen = HashSet::new();
                records
                    .iter()
                    .filter(|r| r.timestamp >= cutoff)
                    .filter_map(|r| {
                        if seen.insert(r.agent_id) {
                            Some(r.agent_id)
                        } else {
                            None
                        }
                    })
                    .collect()
            }
        }
    }

    /// Manually mark a task as poisoned (e.g. by an administrator).
    pub fn force_poison(&self, task_id: Uuid) {
        let mut poisoned = self.poisoned.write().expect("poison set lock poisoned");
        poisoned.insert(task_id);
        info!(task_id = %task_id, "task force-marked POISONED");
    }

    /// Clear poison status for a task (e.g. after a fix has been applied).
    pub fn clear(&self, task_id: &Uuid) {
        {
            let mut poisoned = self.poisoned.write().expect("poison set lock poisoned");
            poisoned.remove(task_id);
        }
        {
            let mut failures = self.failures.write().expect("poison failures lock poisoned");
            failures.remove(task_id);
        }
        info!(task_id = %task_id, "poison status cleared");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(threshold: u32, window_secs: u64) -> FaultConfig {
        FaultConfig {
            poison_agent_threshold: threshold,
            poison_detection_window: std::time::Duration::from_secs(window_secs),
            ..FaultConfig::default()
        }
    }

    #[test]
    fn test_single_failure_is_healthy() {
        let det = PoisonDetector::new(cfg(3, 60));
        let tid = Uuid::new_v4();
        let a1 = AgentId::new();

        let status = det.record_failure(tid, a1);
        assert_eq!(status, PoisonStatus::Healthy);
        assert!(!det.is_poisoned(&tid));
    }

    #[test]
    fn test_two_distinct_agents_is_suspected() {
        let det = PoisonDetector::new(cfg(3, 60));
        let tid = Uuid::new_v4();

        det.record_failure(tid, AgentId::new());
        let status = det.record_failure(tid, AgentId::new());
        assert_eq!(status, PoisonStatus::Suspected);
    }

    #[test]
    fn test_three_distinct_agents_is_poisoned() {
        let det = PoisonDetector::new(cfg(3, 60));
        let tid = Uuid::new_v4();

        det.record_failure(tid, AgentId::new());
        det.record_failure(tid, AgentId::new());
        let status = det.record_failure(tid, AgentId::new());
        assert_eq!(status, PoisonStatus::Poisoned);
        assert!(det.is_poisoned(&tid));
        assert!(det.allow_retry(&tid).is_err());
    }

    #[test]
    fn test_same_agent_multiple_times_not_poison() {
        let det = PoisonDetector::new(cfg(3, 60));
        let tid = Uuid::new_v4();
        let agent = AgentId::new();

        // Same agent failing 5 times is only 1 distinct agent.
        for _ in 0..5 {
            let status = det.record_failure(tid, agent);
            assert_ne!(status, PoisonStatus::Poisoned);
        }
    }

    #[test]
    fn test_force_poison() {
        let det = PoisonDetector::new(cfg(3, 60));
        let tid = Uuid::new_v4();

        det.force_poison(tid);
        assert!(det.is_poisoned(&tid));
        assert!(det.allow_retry(&tid).is_err());
    }

    #[test]
    fn test_clear_poison() {
        let det = PoisonDetector::new(cfg(3, 60));
        let tid = Uuid::new_v4();

        det.force_poison(tid);
        det.clear(&tid);
        assert!(!det.is_poisoned(&tid));
        assert!(det.allow_retry(&tid).is_ok());
    }

    #[test]
    fn test_failed_agents_for() {
        let det = PoisonDetector::new(cfg(5, 60));
        let tid = Uuid::new_v4();
        let a1 = AgentId::new();
        let a2 = AgentId::new();

        det.record_failure(tid, a1);
        det.record_failure(tid, a2);
        det.record_failure(tid, a1); // duplicate

        let agents = det.failed_agents_for(&tid);
        assert_eq!(agents.len(), 2);
    }

    #[test]
    fn test_poisoned_tasks_set() {
        let det = PoisonDetector::new(cfg(1, 60));
        let tid1 = Uuid::new_v4();
        let tid2 = Uuid::new_v4();

        det.record_failure(tid1, AgentId::new());
        det.record_failure(tid2, AgentId::new());

        let set = det.poisoned_tasks();
        assert!(set.contains(&tid1));
        assert!(set.contains(&tid2));
    }

    #[test]
    fn test_status_query_without_recording() {
        let det = PoisonDetector::new(cfg(3, 60));
        let tid = Uuid::new_v4();
        assert_eq!(det.status(&tid), PoisonStatus::Healthy);
    }
}
