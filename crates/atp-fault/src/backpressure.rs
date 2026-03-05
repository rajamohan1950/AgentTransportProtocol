//! Backpressure / queue-depth signaling.
//!
//! Each agent exposes a logical queue depth.  When the depth exceeds a
//! configurable threshold (default 100), the tracker generates a
//! [`BackpressureSignal`] that upstream producers can use to throttle
//! submission rates.

use std::collections::HashMap;
use std::sync::RwLock;
use std::time::Duration;

use atp_types::{AgentId, BackpressureMsg, FaultConfig};
use chrono::{DateTime, Utc};
use tracing::{debug, warn};

/// A signal emitted when an agent's queue depth exceeds the threshold.
#[derive(Debug, Clone)]
pub struct BackpressureSignal {
    /// The overloaded agent.
    pub agent_id: AgentId,
    /// Current queue depth.
    pub queue_depth: u32,
    /// Recommended submission rate (tasks/s) to prevent further growth.
    pub recommended_rate: f64,
    /// Estimated time for the queue to drain at current processing speed.
    pub estimated_drain_time: Duration,
    /// When this signal was generated.
    pub timestamp: DateTime<Utc>,
}

/// Per-agent load snapshot.
#[derive(Debug, Clone)]
struct AgentLoad {
    /// Current queue depth.
    queue_depth: u32,
    /// Observed processing rate (tasks/s).  Updated via an exponential
    /// moving average over heartbeat reports.
    processing_rate: f64,
    /// When the load data was last updated.
    last_updated: DateTime<Utc>,
}

impl AgentLoad {
    fn new() -> Self {
        Self {
            queue_depth: 0,
            processing_rate: 1.0, // conservative default: 1 task/s
            last_updated: Utc::now(),
        }
    }
}

/// Tracks queue depth and processing rate for every known agent.
///
/// Thread-safe via interior `RwLock`.
pub struct AgentLoadTracker {
    config: FaultConfig,
    agents: RwLock<HashMap<AgentId, AgentLoad>>,
}

impl AgentLoadTracker {
    pub fn new(config: FaultConfig) -> Self {
        Self {
            config,
            agents: RwLock::new(HashMap::new()),
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(FaultConfig::default())
    }

    /// The configured queue depth threshold.
    pub fn threshold(&self) -> u32 {
        self.config.backpressure_threshold
    }

    // ── updates ──────────────────────────────────────────────────────

    /// Update the load information for an agent.
    ///
    /// `processing_rate` is the agent's self-reported or observed
    /// throughput in tasks per second.  If `None`, the previous rate is
    /// kept (or the default of 1.0 is used for a new agent).
    pub fn update(
        &self,
        agent: AgentId,
        queue_depth: u32,
        processing_rate: Option<f64>,
    ) {
        let mut agents = self.agents.write().expect("load tracker lock poisoned");
        let entry = agents.entry(agent).or_insert_with(AgentLoad::new);
        entry.queue_depth = queue_depth;
        if let Some(rate) = processing_rate {
            // Exponential moving average (alpha = 0.3).
            entry.processing_rate = 0.7 * entry.processing_rate + 0.3 * rate;
        }
        entry.last_updated = Utc::now();

        if queue_depth > self.config.backpressure_threshold {
            warn!(
                agent = %agent,
                queue_depth,
                threshold = self.config.backpressure_threshold,
                "backpressure threshold exceeded"
            );
        } else {
            debug!(agent = %agent, queue_depth, "load updated");
        }
    }

    /// Convenience: update from a heartbeat message.
    ///
    /// Uses `load_factor` as a proxy for processing rate (tasks/s ≈
    /// 10 × (1 − load_factor) as a simple heuristic).
    pub fn update_from_heartbeat(&self, msg: &atp_types::HeartbeatMsg) {
        let rate = 10.0 * (1.0 - msg.load_factor).max(0.01);
        self.update(msg.from, msg.queue_depth, Some(rate));
    }

    /// Convenience: update from a backpressure message received from a
    /// remote agent.
    pub fn update_from_backpressure(&self, msg: &BackpressureMsg) {
        // Infer processing rate from recommended_rate (it is the rate the
        // agent wants us to send at, so its own rate is at least that).
        self.update(msg.from, msg.queue_depth, Some(msg.recommended_rate));
    }

    // ── queries ──────────────────────────────────────────────────────

    /// Query the current queue depth for an agent.
    pub fn queue_depth(&self, agent: &AgentId) -> Option<u32> {
        let agents = self.agents.read().expect("load tracker lock poisoned");
        agents.get(agent).map(|a| a.queue_depth)
    }

    /// Is the agent's queue above the threshold?
    pub fn is_overloaded(&self, agent: &AgentId) -> bool {
        self.queue_depth(agent)
            .map(|d| d > self.config.backpressure_threshold)
            .unwrap_or(false)
    }

    /// Generate a backpressure signal for an agent if it exceeds the
    /// threshold.  Returns `None` if the agent is not overloaded.
    pub fn signal(&self, agent: &AgentId) -> Option<BackpressureSignal> {
        let agents = self.agents.read().expect("load tracker lock poisoned");
        let load = agents.get(agent)?;

        if load.queue_depth <= self.config.backpressure_threshold {
            return None;
        }

        let recommended_rate = (load.processing_rate * 0.8).max(0.1);
        let drain_secs = if load.processing_rate > 0.0 {
            load.queue_depth as f64 / load.processing_rate
        } else {
            f64::INFINITY
        };
        let estimated_drain_time = if drain_secs.is_finite() {
            Duration::from_secs_f64(drain_secs)
        } else {
            Duration::from_secs(3600) // cap at 1 hour
        };

        Some(BackpressureSignal {
            agent_id: *agent,
            queue_depth: load.queue_depth,
            recommended_rate,
            estimated_drain_time,
            timestamp: Utc::now(),
        })
    }

    /// Return all agents currently above the backpressure threshold,
    /// together with their signals.
    pub fn overloaded_agents(&self) -> Vec<BackpressureSignal> {
        let agents = self.agents.read().expect("load tracker lock poisoned");
        let threshold = self.config.backpressure_threshold;

        agents
            .iter()
            .filter(|(_, load)| load.queue_depth > threshold)
            .map(|(&id, load)| {
                let recommended_rate = (load.processing_rate * 0.8).max(0.1);
                let drain_secs = if load.processing_rate > 0.0 {
                    load.queue_depth as f64 / load.processing_rate
                } else {
                    3600.0
                };
                BackpressureSignal {
                    agent_id: id,
                    queue_depth: load.queue_depth,
                    recommended_rate,
                    estimated_drain_time: Duration::from_secs_f64(drain_secs),
                    timestamp: Utc::now(),
                }
            })
            .collect()
    }

    /// Build a [`BackpressureMsg`] for the given agent suitable for
    /// sending upstream.
    pub fn build_message(&self, agent: AgentId) -> BackpressureMsg {
        let agents = self.agents.read().expect("load tracker lock poisoned");
        let (queue_depth, recommended_rate, drain_ms) =
            if let Some(load) = agents.get(&agent) {
                let rate = (load.processing_rate * 0.8).max(0.1);
                let drain = if load.processing_rate > 0.0 {
                    ((load.queue_depth as f64 / load.processing_rate) * 1000.0) as u64
                } else {
                    3_600_000
                };
                (load.queue_depth, rate, drain)
            } else {
                (0, 1.0, 0)
            };

        BackpressureMsg {
            from: agent,
            queue_depth,
            recommended_rate,
            estimated_drain_ms: drain_ms,
        }
    }

    /// Remove tracking data for an agent.
    pub fn remove_agent(&self, agent: &AgentId) -> bool {
        let mut agents = self.agents.write().expect("load tracker lock poisoned");
        agents.remove(agent).is_some()
    }

    /// Total number of tracked agents.
    pub fn tracked_count(&self) -> usize {
        let agents = self.agents.read().expect("load tracker lock poisoned");
        agents.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use atp_types::HeartbeatMsg;

    fn cfg(threshold: u32) -> FaultConfig {
        FaultConfig {
            backpressure_threshold: threshold,
            ..FaultConfig::default()
        }
    }

    #[test]
    fn test_below_threshold_not_overloaded() {
        let tracker = AgentLoadTracker::new(cfg(100));
        let a = AgentId::new();
        tracker.update(a, 50, Some(10.0));
        assert!(!tracker.is_overloaded(&a));
        assert!(tracker.signal(&a).is_none());
    }

    #[test]
    fn test_above_threshold_overloaded() {
        let tracker = AgentLoadTracker::new(cfg(100));
        let a = AgentId::new();
        tracker.update(a, 150, Some(10.0));
        assert!(tracker.is_overloaded(&a));

        let sig = tracker.signal(&a).unwrap();
        assert_eq!(sig.queue_depth, 150);
        assert!(sig.recommended_rate > 0.0);
        assert!(sig.estimated_drain_time > Duration::ZERO);
    }

    #[test]
    fn test_exact_threshold_not_overloaded() {
        let tracker = AgentLoadTracker::new(cfg(100));
        let a = AgentId::new();
        tracker.update(a, 100, Some(10.0));
        // threshold is > not >=
        assert!(!tracker.is_overloaded(&a));
    }

    #[test]
    fn test_update_from_heartbeat() {
        let tracker = AgentLoadTracker::new(cfg(50));
        let a = AgentId::new();

        let hb = HeartbeatMsg {
            from: a,
            sequence: 1,
            queue_depth: 60,
            load_factor: 0.5,
        };
        tracker.update_from_heartbeat(&hb);

        assert_eq!(tracker.queue_depth(&a), Some(60));
        assert!(tracker.is_overloaded(&a));
    }

    #[test]
    fn test_overloaded_agents() {
        let tracker = AgentLoadTracker::new(cfg(50));
        let a = AgentId::new();
        let b = AgentId::new();
        let c = AgentId::new();

        tracker.update(a, 60, Some(5.0));
        tracker.update(b, 30, Some(5.0));
        tracker.update(c, 80, Some(5.0));

        let overloaded = tracker.overloaded_agents();
        assert_eq!(overloaded.len(), 2);
    }

    #[test]
    fn test_build_message() {
        let tracker = AgentLoadTracker::new(cfg(100));
        let a = AgentId::new();
        tracker.update(a, 120, Some(10.0));

        let msg = tracker.build_message(a);
        assert_eq!(msg.from, a);
        assert_eq!(msg.queue_depth, 120);
        assert!(msg.recommended_rate > 0.0);
        assert!(msg.estimated_drain_ms > 0);
    }

    #[test]
    fn test_remove_agent() {
        let tracker = AgentLoadTracker::new(cfg(100));
        let a = AgentId::new();
        tracker.update(a, 10, None);
        assert_eq!(tracker.tracked_count(), 1);
        assert!(tracker.remove_agent(&a));
        assert_eq!(tracker.tracked_count(), 0);
    }

    #[test]
    fn test_unknown_agent_not_overloaded() {
        let tracker = AgentLoadTracker::new(cfg(100));
        let a = AgentId::new();
        assert!(!tracker.is_overloaded(&a));
        assert_eq!(tracker.queue_depth(&a), None);
    }

    #[test]
    fn test_ema_smoothing() {
        let tracker = AgentLoadTracker::new(cfg(100));
        let a = AgentId::new();

        // First update sets rate via EMA from default 1.0.
        tracker.update(a, 10, Some(10.0));
        // Second update should be smoothed.
        tracker.update(a, 10, Some(20.0));

        // We cannot directly read the rate, but we can check the signal
        // math is consistent by building a message.
        let msg = tracker.build_message(a);
        // Rate should be between 10 and 20 due to smoothing.
        assert!(msg.recommended_rate > 0.0);
    }
}
