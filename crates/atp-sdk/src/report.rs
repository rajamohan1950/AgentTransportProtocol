use atp_sim::BenchMetrics;

/// A benchmark report. Print it for a beautiful table. That's all you need.
///
/// ```text
/// ════════════════════════════════════════════════════════════════
///   ATP Benchmark: 50 agents, 10000 tasks, seed=42
/// ════════════════════════════════════════════════════════════════
///
/// Scenario             Cost/Task  Latency  Quality  Recovery    Ctx  Failed
/// ────────────────────────────────────────────────────────────────────────
/// Sequential            $0.0844    800ms    0.837       inf   1.0x       0
/// Round-Robin            ...
/// ATP (full)            $0.0393    568ms    0.904      0ms  28.0x       0
///   ...
/// ────────────────────────────────────────────────────────────────────────
///
///   ATP vs Sequential:
///     Cost:    -53.4%
///     Latency: -29.0%
///     Quality: +0.067
/// ```
#[derive(Debug, Clone)]
pub struct BenchReport {
    metrics: Vec<BenchMetrics>,
    agents: usize,
    tasks: usize,
    seed: u64,
}

impl BenchReport {
    pub(crate) fn new(
        metrics: Vec<BenchMetrics>,
        agents: usize,
        tasks: usize,
        seed: u64,
    ) -> Self {
        Self {
            metrics,
            agents,
            tasks,
            seed,
        }
    }

    /// Get metrics for a specific scenario by name substring (case-insensitive).
    pub fn scenario(&self, name: &str) -> Option<&BenchMetrics> {
        let lower = name.to_lowercase();
        self.metrics
            .iter()
            .find(|m| m.scenario.to_lowercase().contains(&lower))
    }

    /// Get the full ATP metrics.
    pub fn atp(&self) -> Option<&BenchMetrics> {
        self.scenario("ATP (full)")
    }

    /// Get the sequential baseline metrics.
    pub fn baseline(&self) -> Option<&BenchMetrics> {
        self.scenario("Sequential")
    }

    /// All scenario metrics.
    pub fn all(&self) -> &[BenchMetrics] {
        &self.metrics
    }
}

impl std::fmt::Display for BenchReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let bar = "═".repeat(68);
        let dash = "─".repeat(68);

        writeln!(f, "{bar}")?;
        writeln!(
            f,
            "  ATP Benchmark: {} agents, {} tasks, seed={}",
            self.agents, self.tasks, self.seed
        )?;
        writeln!(f, "{bar}")?;
        writeln!(f)?;

        // Header
        writeln!(
            f,
            "{:<20} {:>9} {:>8} {:>8} {:>9} {:>6} {:>6}",
            "Scenario", "Cost/Task", "Latency", "Quality", "Recovery", "Ctx", "Failed"
        )?;
        writeln!(f, "{dash}")?;

        // Rows
        for m in &self.metrics {
            let recovery = if m.fault_recovery_ms.is_infinite() {
                "inf".to_string()
            } else {
                format!("{:.0}ms", m.fault_recovery_ms)
            };
            let ctx = 1.0 / m.context_efficiency.max(0.001);

            writeln!(
                f,
                "{:<20} ${:>7.4} {:>6.0}ms {:>7.3} {:>9} {:>5.1}x {:>6}",
                m.scenario,
                m.avg_cost_per_task,
                m.avg_latency_ms,
                m.avg_quality,
                recovery,
                ctx,
                m.tasks_failed,
            )?;
        }
        writeln!(f, "{dash}")?;

        // ATP vs Sequential comparison
        if let (Some(seq), Some(atp)) = (self.baseline(), self.atp()) {
            writeln!(f)?;
            writeln!(f, "  ATP vs Sequential:")?;
            if seq.avg_cost_per_task > 0.0 {
                let cost_reduction =
                    (1.0 - atp.avg_cost_per_task / seq.avg_cost_per_task) * 100.0;
                writeln!(f, "    Cost:    {cost_reduction:+.1}%")?;
            }
            if seq.avg_latency_ms > 0.0 {
                let latency_reduction =
                    (1.0 - atp.avg_latency_ms / seq.avg_latency_ms) * 100.0;
                writeln!(f, "    Latency: {latency_reduction:+.1}%")?;
            }
            let quality_delta = atp.avg_quality - seq.avg_quality;
            writeln!(f, "    Quality: {quality_delta:+.3}")?;
        }

        Ok(())
    }
}
