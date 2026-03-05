//! Heartbeat protocol with phi-accrual–style failure detection.
//!
//! Default interval: 1 s.  Timeout = 3 × interval (configurable via
//! [`FaultConfig::heartbeat_timeout_multiplier`]).  Detection latency
//! T_fail < 100 ms after the timeout expires (bounded by the check loop
//! granularity).

use std::collections::HashMap;
use std::sync::RwLock;
use std::time::Duration;

use atp_types::{AgentId, FaultConfig, FaultError, HeartbeatMsg};
use chrono::{DateTime, Utc};
use tracing::{debug, warn};

/// Per-agent health status returned by the monitor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeartbeatStatus {
    /// Agent is alive — last heartbeat within the timeout window.
    Alive,
    /// Agent has not responded within the timeout window.
    Suspected,
    /// Agent has been declared dead (sustained silence beyond timeout).
    Dead,
}

/// Internal bookkeeping for a single agent.
#[derive(Debug, Clone)]
struct AgentHeartbeatState {
    /// Monotonically increasing sequence number from the remote agent.
    last_sequence: u64,
    /// Wall-clock time of the last received heartbeat.
    last_seen: DateTime<Utc>,
    /// Most recent queue depth reported by the agent.
    queue_depth: u32,
    /// Most recent load factor reported by the agent.
    load_factor: f64,
    /// Current health verdict.
    status: HeartbeatStatus,
}

/// Tracks heartbeat reception for all known agents and decides liveness.
///
/// Thread-safe via interior `RwLock`.  Designed for use from both the
/// async receive path (`record_heartbeat`) and a periodic check task
/// (`check_all`).
pub struct HeartbeatMonitor {
    config: FaultConfig,
    agents: RwLock<HashMap<AgentId, AgentHeartbeatState>>,
}

impl HeartbeatMonitor {
    /// Create a new monitor with the given fault configuration.
    pub fn new(config: FaultConfig) -> Self {
        Self {
            config,
            agents: RwLock::new(HashMap::new()),
        }
    }

    /// Create a monitor with default configuration.
    pub fn with_defaults() -> Self {
        Self::new(FaultConfig::default())
    }

    /// Computed timeout duration (interval × multiplier).
    pub fn timeout(&self) -> Duration {
        self.config.heartbeat_interval * self.config.heartbeat_timeout_multiplier
    }

    /// Record an incoming heartbeat from an agent.
    ///
    /// If this is the first heartbeat from the agent the entry is created
    /// automatically.
    pub fn record_heartbeat(&self, msg: &HeartbeatMsg) {
        let now = Utc::now();
        let mut agents = self.agents.write().expect("heartbeat lock poisoned");
        let entry = agents.entry(msg.from).or_insert_with(|| {
            debug!(agent = %msg.from, "new agent registered via heartbeat");
            AgentHeartbeatState {
                last_sequence: 0,
                last_seen: now,
                queue_depth: 0,
                load_factor: 0.0,
                status: HeartbeatStatus::Alive,
            }
        });

        // Accept only monotonically increasing sequence numbers.
        if msg.sequence > entry.last_sequence {
            entry.last_sequence = msg.sequence;
            entry.last_seen = now;
            entry.queue_depth = msg.queue_depth;
            entry.load_factor = msg.load_factor;
            entry.status = HeartbeatStatus::Alive;
        }
    }

    /// Evaluate all tracked agents and return those whose status changed to
    /// `Suspected` or `Dead` since the last check.
    ///
    /// This method is intended to be called on a tight timer (e.g. every
    /// 50–100 ms) to guarantee T_fail < 100 ms.
    pub fn check_all(&self) -> Vec<(AgentId, HeartbeatStatus)> {
        let now = Utc::now();
        let timeout = chrono::Duration::from_std(self.timeout())
            .unwrap_or_else(|_| chrono::Duration::seconds(3));

        let mut changed = Vec::new();
        let mut agents = self.agents.write().expect("heartbeat lock poisoned");

        for (&id, state) in agents.iter_mut() {
            let elapsed = now.signed_duration_since(state.last_seen);

            let new_status = if elapsed <= timeout {
                HeartbeatStatus::Alive
            } else if elapsed <= timeout + timeout {
                // Between 1× and 2× timeout → suspected
                HeartbeatStatus::Suspected
            } else {
                HeartbeatStatus::Dead
            };

            if new_status != state.status {
                let old = state.status;
                state.status = new_status;
                changed.push((id, new_status));
                warn!(
                    agent = %id,
                    ?old,
                    ?new_status,
                    "heartbeat status changed"
                );
            }
        }

        changed
    }

    /// Query the current status of a single agent.
    pub fn status(&self, agent: &AgentId) -> Result<HeartbeatStatus, FaultError> {
        let agents = self.agents.read().expect("heartbeat lock poisoned");
        agents
            .get(agent)
            .map(|s| s.status)
            .ok_or_else(|| FaultError::HeartbeatTimeout(agent.to_string()))
    }

    /// Return the last-seen timestamp for an agent, if known.
    pub fn last_seen(&self, agent: &AgentId) -> Option<DateTime<Utc>> {
        let agents = self.agents.read().expect("heartbeat lock poisoned");
        agents.get(agent).map(|s| s.last_seen)
    }

    /// Return the queue depth most recently reported by an agent.
    pub fn queue_depth(&self, agent: &AgentId) -> Option<u32> {
        let agents = self.agents.read().expect("heartbeat lock poisoned");
        agents.get(agent).map(|s| s.queue_depth)
    }

    /// List all agents currently considered alive.
    pub fn alive_agents(&self) -> Vec<AgentId> {
        let agents = self.agents.read().expect("heartbeat lock poisoned");
        agents
            .iter()
            .filter(|(_, s)| s.status == HeartbeatStatus::Alive)
            .map(|(&id, _)| id)
            .collect()
    }

    /// List all agents currently suspected or dead.
    pub fn failed_agents(&self) -> Vec<(AgentId, HeartbeatStatus)> {
        let agents = self.agents.read().expect("heartbeat lock poisoned");
        agents
            .iter()
            .filter(|(_, s)| s.status != HeartbeatStatus::Alive)
            .map(|(&id, s)| (id, s.status))
            .collect()
    }

    /// Remove an agent from tracking entirely (e.g. after confirmed departure).
    pub fn remove_agent(&self, agent: &AgentId) -> bool {
        let mut agents = self.agents.write().expect("heartbeat lock poisoned");
        agents.remove(agent).is_some()
    }

    /// Total number of tracked agents.
    pub fn tracked_count(&self) -> usize {
        let agents = self.agents.read().expect("heartbeat lock poisoned");
        agents.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    fn make_config(interval_ms: u64, multiplier: u32) -> FaultConfig {
        FaultConfig {
            heartbeat_interval: Duration::from_millis(interval_ms),
            heartbeat_timeout_multiplier: multiplier,
            ..FaultConfig::default()
        }
    }

    fn hb(agent: AgentId, seq: u64) -> HeartbeatMsg {
        HeartbeatMsg {
            from: agent,
            sequence: seq,
            queue_depth: 0,
            load_factor: 0.0,
        }
    }

    #[test]
    fn test_record_and_alive() {
        let mon = HeartbeatMonitor::new(make_config(1000, 3));
        let a = AgentId::new();
        mon.record_heartbeat(&hb(a, 1));
        assert_eq!(mon.status(&a).unwrap(), HeartbeatStatus::Alive);
        assert_eq!(mon.tracked_count(), 1);
    }

    #[test]
    fn test_timeout_transitions_to_suspected() {
        // Use a very short interval so we can observe the transition quickly.
        let mon = HeartbeatMonitor::new(make_config(50, 1)); // timeout = 50ms
        let a = AgentId::new();
        mon.record_heartbeat(&hb(a, 1));

        // Wait for timeout to expire.
        thread::sleep(Duration::from_millis(80));
        let changed = mon.check_all();
        assert!(!changed.is_empty());
        assert_eq!(mon.status(&a).unwrap(), HeartbeatStatus::Suspected);
    }

    #[test]
    fn test_dead_after_extended_silence() {
        let mon = HeartbeatMonitor::new(make_config(20, 1)); // timeout = 20ms
        let a = AgentId::new();
        mon.record_heartbeat(&hb(a, 1));

        // Wait > 2× timeout.
        thread::sleep(Duration::from_millis(60));
        let _ = mon.check_all();
        assert_eq!(mon.status(&a).unwrap(), HeartbeatStatus::Dead);
    }

    #[test]
    fn test_recovery_on_heartbeat() {
        let mon = HeartbeatMonitor::new(make_config(20, 1));
        let a = AgentId::new();
        mon.record_heartbeat(&hb(a, 1));

        thread::sleep(Duration::from_millis(40));
        let _ = mon.check_all();
        assert_ne!(mon.status(&a).unwrap(), HeartbeatStatus::Alive);

        // Agent recovers.
        mon.record_heartbeat(&hb(a, 2));
        assert_eq!(mon.status(&a).unwrap(), HeartbeatStatus::Alive);
    }

    #[test]
    fn test_duplicate_sequence_ignored() {
        let mon = HeartbeatMonitor::new(make_config(1000, 3));
        let a = AgentId::new();
        mon.record_heartbeat(&hb(a, 5));
        let t1 = mon.last_seen(&a).unwrap();

        thread::sleep(Duration::from_millis(10));
        // Replay an old sequence — should be ignored.
        mon.record_heartbeat(&hb(a, 3));
        let t2 = mon.last_seen(&a).unwrap();
        assert_eq!(t1, t2);
    }

    #[test]
    fn test_remove_agent() {
        let mon = HeartbeatMonitor::new(make_config(1000, 3));
        let a = AgentId::new();
        mon.record_heartbeat(&hb(a, 1));
        assert!(mon.remove_agent(&a));
        assert!(mon.status(&a).is_err());
    }
}
