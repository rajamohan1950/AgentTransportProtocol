//! Three-state circuit breaker: CLOSED -> OPEN -> HALF_OPEN -> CLOSED.
//!
//! State machine:
//!   CLOSED   — normal operation, failures are counted.
//!   OPEN     — requests are rejected immediately; entered after
//!              `N_fail` (default 3) consecutive failures.
//!   HALF_OPEN — entered after a cool-down period (default 30 s);
//!              allows exactly one probe request.
//!              * probe succeeds → CLOSED (counter reset)
//!              * probe fails   → OPEN   (cool-down restarts)

use std::collections::HashMap;
use std::sync::RwLock;
use std::time::Duration;

use atp_types::{AgentId, CircuitBreakMsg, CircuitState, FaultConfig, FaultError};
use chrono::{DateTime, Utc};
use tracing::{info, warn};

/// Per-agent circuit breaker bookkeeping.
#[derive(Debug, Clone)]
struct BreakerState {
    state: CircuitState,
    consecutive_failures: u32,
    last_state_change: DateTime<Utc>,
    /// Total lifetime failure count (for metrics).
    total_failures: u64,
    /// Total lifetime success count (for metrics).
    total_successes: u64,
}

impl BreakerState {
    fn new() -> Self {
        Self {
            state: CircuitState::Closed,
            consecutive_failures: 0,
            last_state_change: Utc::now(),
            total_failures: 0,
            total_successes: 0,
        }
    }
}

/// Manages circuit breaker state for every known remote agent.
///
/// Thread-safe via interior `RwLock`.
pub struct CircuitBreaker {
    config: FaultConfig,
    /// Cool-down period before transitioning OPEN → HALF_OPEN.
    cooldown: Duration,
    breakers: RwLock<HashMap<AgentId, BreakerState>>,
}

impl CircuitBreaker {
    /// Create a circuit breaker manager with explicit cool-down.
    pub fn new(config: FaultConfig, cooldown: Duration) -> Self {
        Self {
            config,
            cooldown,
            breakers: RwLock::new(HashMap::new()),
        }
    }

    /// Create a circuit breaker manager with the default 30 s cool-down.
    pub fn with_defaults() -> Self {
        Self::new(FaultConfig::default(), Duration::from_secs(30))
    }

    // ── queries ──────────────────────────────────────────────────────

    /// Check whether a request to `agent` should be allowed.
    ///
    /// Returns `Ok(())` when the circuit is CLOSED or HALF_OPEN (probe
    /// allowed), and `Err(FaultError::CircuitOpen)` when OPEN.
    ///
    /// As a side-effect, an OPEN breaker whose cool-down has elapsed is
    /// transitioned to HALF_OPEN so the next call sees the probe state.
    pub fn allow_request(&self, agent: &AgentId) -> Result<(), FaultError> {
        let mut breakers = self.breakers.write().expect("circuit breaker lock poisoned");
        let entry = breakers.entry(*agent).or_insert_with(BreakerState::new);

        match entry.state {
            CircuitState::Closed => Ok(()),
            CircuitState::HalfOpen => {
                // One probe is allowed — the caller must subsequently call
                // `record_success` or `record_failure`.
                Ok(())
            }
            CircuitState::Open => {
                let elapsed = Utc::now().signed_duration_since(entry.last_state_change);
                let cooldown_chrono = chrono::Duration::from_std(self.cooldown)
                    .unwrap_or_else(|_| chrono::Duration::seconds(30));

                if elapsed >= cooldown_chrono {
                    // Transition to HALF_OPEN.
                    entry.state = CircuitState::HalfOpen;
                    entry.last_state_change = Utc::now();
                    info!(agent = %agent, "circuit breaker: OPEN -> HALF_OPEN (cooldown elapsed)");
                    Ok(())
                } else {
                    Err(FaultError::CircuitOpen(agent.to_string()))
                }
            }
        }
    }

    /// Query the current circuit state for an agent.
    pub fn state(&self, agent: &AgentId) -> CircuitState {
        let breakers = self.breakers.read().expect("circuit breaker lock poisoned");
        breakers
            .get(agent)
            .map(|s| s.state)
            .unwrap_or(CircuitState::Closed)
    }

    /// Return the number of consecutive failures for an agent.
    pub fn failure_count(&self, agent: &AgentId) -> u32 {
        let breakers = self.breakers.read().expect("circuit breaker lock poisoned");
        breakers.get(agent).map(|s| s.consecutive_failures).unwrap_or(0)
    }

    // ── mutations ────────────────────────────────────────────────────

    /// Record a successful interaction with `agent`.
    ///
    /// * HALF_OPEN → CLOSED (probe succeeded — reset counters)
    /// * CLOSED    → stays CLOSED (counter reset)
    pub fn record_success(&self, agent: &AgentId) {
        let mut breakers = self.breakers.write().expect("circuit breaker lock poisoned");
        let entry = breakers.entry(*agent).or_insert_with(BreakerState::new);

        entry.total_successes += 1;

        match entry.state {
            CircuitState::HalfOpen => {
                info!(agent = %agent, "circuit breaker: HALF_OPEN -> CLOSED (probe succeeded)");
                entry.state = CircuitState::Closed;
                entry.consecutive_failures = 0;
                entry.last_state_change = Utc::now();
            }
            CircuitState::Closed => {
                entry.consecutive_failures = 0;
            }
            CircuitState::Open => {
                // Unexpected success while open — treat conservatively.
                warn!(agent = %agent, "success recorded while circuit is OPEN; ignoring");
            }
        }
    }

    /// Record a failed interaction with `agent`.
    ///
    /// * CLOSED    → increments failure count; trips to OPEN at threshold
    /// * HALF_OPEN → OPEN (probe failed — restart cool-down)
    /// * OPEN      → counter incremented but no state change
    pub fn record_failure(&self, agent: &AgentId) {
        let mut breakers = self.breakers.write().expect("circuit breaker lock poisoned");
        let entry = breakers.entry(*agent).or_insert_with(BreakerState::new);

        entry.consecutive_failures += 1;
        entry.total_failures += 1;

        match entry.state {
            CircuitState::Closed => {
                if entry.consecutive_failures >= self.config.circuit_breaker_threshold {
                    warn!(
                        agent = %agent,
                        failures = entry.consecutive_failures,
                        "circuit breaker: CLOSED -> OPEN (threshold reached)"
                    );
                    entry.state = CircuitState::Open;
                    entry.last_state_change = Utc::now();
                }
            }
            CircuitState::HalfOpen => {
                warn!(agent = %agent, "circuit breaker: HALF_OPEN -> OPEN (probe failed)");
                entry.state = CircuitState::Open;
                entry.last_state_change = Utc::now();
            }
            CircuitState::Open => {
                // Already open — nothing to do.
            }
        }
    }

    /// Forcibly reset the breaker for an agent back to CLOSED.
    pub fn reset(&self, agent: &AgentId) {
        let mut breakers = self.breakers.write().expect("circuit breaker lock poisoned");
        if let Some(entry) = breakers.get_mut(agent) {
            info!(agent = %agent, "circuit breaker: forced reset to CLOSED");
            entry.state = CircuitState::Closed;
            entry.consecutive_failures = 0;
            entry.last_state_change = Utc::now();
        }
    }

    /// Build a [`CircuitBreakMsg`] reflecting the current state of `agent`.
    pub fn build_message(&self, from: AgentId, target: AgentId) -> CircuitBreakMsg {
        let breakers = self.breakers.read().expect("circuit breaker lock poisoned");
        let (state, failure_count) = breakers
            .get(&target)
            .map(|s| (s.state, s.consecutive_failures))
            .unwrap_or((CircuitState::Closed, 0));

        CircuitBreakMsg {
            from,
            target,
            state,
            failure_count,
        }
    }

    /// Return all agents whose circuit is currently OPEN.
    pub fn open_circuits(&self) -> Vec<AgentId> {
        let breakers = self.breakers.read().expect("circuit breaker lock poisoned");
        breakers
            .iter()
            .filter(|(_, s)| s.state == CircuitState::Open)
            .map(|(&id, _)| id)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(threshold: u32) -> FaultConfig {
        FaultConfig {
            circuit_breaker_threshold: threshold,
            ..FaultConfig::default()
        }
    }

    #[test]
    fn test_starts_closed() {
        let cb = CircuitBreaker::with_defaults();
        let a = AgentId::new();
        assert_eq!(cb.state(&a), CircuitState::Closed);
        assert!(cb.allow_request(&a).is_ok());
    }

    #[test]
    fn test_trips_open_after_threshold() {
        let cb = CircuitBreaker::new(cfg(3), Duration::from_secs(30));
        let a = AgentId::new();

        cb.record_failure(&a);
        assert_eq!(cb.state(&a), CircuitState::Closed);

        cb.record_failure(&a);
        assert_eq!(cb.state(&a), CircuitState::Closed);

        cb.record_failure(&a);
        assert_eq!(cb.state(&a), CircuitState::Open);
        assert!(cb.allow_request(&a).is_err());
    }

    #[test]
    fn test_success_resets_failure_count() {
        let cb = CircuitBreaker::new(cfg(3), Duration::from_secs(30));
        let a = AgentId::new();

        cb.record_failure(&a);
        cb.record_failure(&a);
        assert_eq!(cb.failure_count(&a), 2);

        cb.record_success(&a);
        assert_eq!(cb.failure_count(&a), 0);
        assert_eq!(cb.state(&a), CircuitState::Closed);
    }

    #[test]
    fn test_half_open_probe_success() {
        // Use zero cooldown to immediately transition OPEN → HALF_OPEN.
        let cb = CircuitBreaker::new(cfg(1), Duration::from_millis(0));
        let a = AgentId::new();

        cb.record_failure(&a);
        assert_eq!(cb.state(&a), CircuitState::Open);

        // Cooldown is zero, so allow_request should transition to HALF_OPEN.
        std::thread::sleep(Duration::from_millis(5));
        assert!(cb.allow_request(&a).is_ok());
        assert_eq!(cb.state(&a), CircuitState::HalfOpen);

        // Probe succeeds.
        cb.record_success(&a);
        assert_eq!(cb.state(&a), CircuitState::Closed);
    }

    #[test]
    fn test_half_open_probe_failure() {
        let cb = CircuitBreaker::new(cfg(1), Duration::from_millis(0));
        let a = AgentId::new();

        cb.record_failure(&a);

        std::thread::sleep(Duration::from_millis(5));
        assert!(cb.allow_request(&a).is_ok()); // HALF_OPEN
        assert_eq!(cb.state(&a), CircuitState::HalfOpen);

        // Probe fails — back to OPEN.
        cb.record_failure(&a);
        assert_eq!(cb.state(&a), CircuitState::Open);
    }

    #[test]
    fn test_force_reset() {
        let cb = CircuitBreaker::new(cfg(1), Duration::from_secs(300));
        let a = AgentId::new();

        cb.record_failure(&a);
        assert_eq!(cb.state(&a), CircuitState::Open);

        cb.reset(&a);
        assert_eq!(cb.state(&a), CircuitState::Closed);
        assert!(cb.allow_request(&a).is_ok());
    }

    #[test]
    fn test_open_circuits_list() {
        let cb = CircuitBreaker::new(cfg(1), Duration::from_secs(300));
        let a = AgentId::new();
        let b = AgentId::new();

        cb.record_failure(&a);
        cb.record_failure(&b);

        let open = cb.open_circuits();
        assert_eq!(open.len(), 2);
    }

    #[test]
    fn test_build_message() {
        let cb = CircuitBreaker::new(cfg(1), Duration::from_secs(300));
        let me = AgentId::new();
        let target = AgentId::new();

        cb.record_failure(&target);
        let msg = cb.build_message(me, target);
        assert_eq!(msg.state, CircuitState::Open);
        assert_eq!(msg.failure_count, 1);
        assert_eq!(msg.from, me);
        assert_eq!(msg.target, target);
    }
}
