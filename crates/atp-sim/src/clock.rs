use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// Deterministic simulated clock for reproducible benchmarks.
/// All time advances are explicit -- no wall-clock dependency.
#[derive(Debug, Clone)]
pub struct SimulatedClock {
    /// Nanoseconds since epoch.
    nanos: Arc<AtomicU64>,
}

impl SimulatedClock {
    pub fn new() -> Self {
        // Start at a fixed epoch: 2026-01-01T00:00:00Z
        let epoch_nanos = 1_767_225_600_000_000_000u64;
        Self {
            nanos: Arc::new(AtomicU64::new(epoch_nanos)),
        }
    }

    pub fn now_nanos(&self) -> u64 {
        self.nanos.load(Ordering::SeqCst)
    }

    pub fn now_chrono(&self) -> chrono::DateTime<chrono::Utc> {
        let nanos = self.now_nanos();
        let secs = (nanos / 1_000_000_000) as i64;
        let nsecs = (nanos % 1_000_000_000) as u32;
        chrono::DateTime::from_timestamp(secs, nsecs).unwrap_or_default()
    }

    pub fn advance(&self, duration: Duration) {
        self.nanos
            .fetch_add(duration.as_nanos() as u64, Ordering::SeqCst);
    }

    pub fn advance_ms(&self, ms: u64) {
        self.advance(Duration::from_millis(ms));
    }

    pub fn elapsed_since(&self, start_nanos: u64) -> Duration {
        let now = self.now_nanos();
        Duration::from_nanos(now.saturating_sub(start_nanos))
    }
}

impl Default for SimulatedClock {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clock_advance() {
        let clock = SimulatedClock::new();
        let t0 = clock.now_nanos();
        clock.advance(Duration::from_secs(1));
        let t1 = clock.now_nanos();
        assert_eq!(t1 - t0, 1_000_000_000);
    }

    #[test]
    fn test_clock_clone_shares_state() {
        let c1 = SimulatedClock::new();
        let c2 = c1.clone();
        c1.advance(Duration::from_secs(5));
        assert_eq!(c1.now_nanos(), c2.now_nanos());
    }
}
