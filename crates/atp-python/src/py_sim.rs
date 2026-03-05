use pyo3::prelude::*;
use atp_sim::{Scenario, SimHarness, TaskGenerator};

/// Benchmark metrics from a simulation run.
#[pyclass]
#[derive(Clone)]
pub struct PyBenchMetrics {
    #[pyo3(get)]
    pub scenario: String,
    #[pyo3(get)]
    pub total_tasks: usize,
    #[pyo3(get)]
    pub tasks_completed: usize,
    #[pyo3(get)]
    pub tasks_failed: usize,
    #[pyo3(get)]
    pub avg_cost_per_task: f64,
    #[pyo3(get)]
    pub avg_latency_ms: f64,
    #[pyo3(get)]
    pub p50_latency_ms: f64,
    #[pyo3(get)]
    pub p95_latency_ms: f64,
    #[pyo3(get)]
    pub p99_latency_ms: f64,
    #[pyo3(get)]
    pub avg_quality: f64,
    #[pyo3(get)]
    pub fault_recovery_ms: f64,
    #[pyo3(get)]
    pub context_efficiency: f64,
}

#[pymethods]
impl PyBenchMetrics {
    fn __repr__(&self) -> String {
        format!(
            "BenchMetrics(scenario='{}', cost=${:.4}, latency={:.0}ms, quality={:.3}, completed={}, failed={})",
            self.scenario, self.avg_cost_per_task, self.avg_latency_ms,
            self.avg_quality, self.tasks_completed, self.tasks_failed
        )
    }
}

/// ATP simulation and benchmarking framework.
///
/// Example:
///     sim = atp.Simulation(agents=50, seed=42)
///     results = sim.run_benchmark(tasks=10000)
///     for r in results:
///         print(r)
#[pyclass]
pub struct PySimulation {
    harness: SimHarness,
    seed: u64,
}

#[pymethods]
impl PySimulation {
    /// Create a new simulation with the benchmark network.
    #[new]
    #[pyo3(signature = (agents=50, seed=42))]
    fn new(agents: usize, seed: u64) -> Self {
        let _ = agents; // Currently uses fixed 50-agent benchmark topology
        Self {
            harness: SimHarness::benchmark(seed),
            seed,
        }
    }

    /// Number of agents in the network.
    fn agent_count(&self) -> usize {
        self.harness.network.agent_count()
    }

    /// Number of edges in the network topology.
    fn edge_count(&self) -> usize {
        self.harness.network.edges.len()
    }

    /// Run a single scenario.
    #[pyo3(signature = (scenario="atp", tasks=10000, context_size=50000))]
    fn run_scenario(&self, scenario: &str, tasks: usize, context_size: usize) -> PyResult<PyBenchMetrics> {
        let scenario_enum = parse_scenario(scenario)?;
        let mut rng = <rand::rngs::StdRng as rand::SeedableRng>::seed_from_u64(self.seed);
        let generator = TaskGenerator::new().with_context_size(context_size);
        let task_list = generator.generate(tasks, &mut rng);
        let metrics = self.harness.run_scenario(scenario_enum, &task_list);
        Ok(to_py_metrics(&metrics))
    }

    /// Run all 7 benchmark scenarios.
    #[pyo3(signature = (tasks=10000, context_size=50000))]
    fn run_benchmark(&self, tasks: usize, context_size: usize) -> PyResult<Vec<PyBenchMetrics>> {
        let mut rng = <rand::rngs::StdRng as rand::SeedableRng>::seed_from_u64(self.seed);
        let generator = TaskGenerator::new().with_context_size(context_size);
        let task_list = generator.generate(tasks, &mut rng);

        let scenarios = vec![
            Scenario::Sequential,
            Scenario::RoundRobin,
            Scenario::FullAtp,
            Scenario::AtpNoContext,
            Scenario::AtpNoRouting,
            Scenario::AtpNoTrust,
            Scenario::AtpNoFault,
        ];

        let results = scenarios
            .iter()
            .map(|s| {
                let metrics = self.harness.run_scenario(*s, &task_list);
                to_py_metrics(&metrics)
            })
            .collect();

        Ok(results)
    }

    fn __repr__(&self) -> String {
        format!(
            "Simulation(agents={}, edges={}, seed={})",
            self.agent_count(),
            self.edge_count(),
            self.seed
        )
    }
}

fn parse_scenario(name: &str) -> PyResult<Scenario> {
    match name.to_lowercase().as_str() {
        "sequential" => Ok(Scenario::Sequential),
        "roundrobin" | "round_robin" => Ok(Scenario::RoundRobin),
        "atp" | "full" | "full_atp" => Ok(Scenario::FullAtp),
        "nocontext" | "no_context" => Ok(Scenario::AtpNoContext),
        "norouting" | "no_routing" => Ok(Scenario::AtpNoRouting),
        "notrust" | "no_trust" => Ok(Scenario::AtpNoTrust),
        "nofault" | "no_fault" => Ok(Scenario::AtpNoFault),
        _ => Err(pyo3::exceptions::PyValueError::new_err(
            format!("Unknown scenario: {name}. Options: sequential, roundrobin, atp, nocontext, norouting, notrust, nofault")
        )),
    }
}

fn to_py_metrics(m: &atp_sim::BenchMetrics) -> PyBenchMetrics {
    PyBenchMetrics {
        scenario: m.scenario.clone(),
        total_tasks: m.total_tasks,
        tasks_completed: m.tasks_completed,
        tasks_failed: m.tasks_failed,
        avg_cost_per_task: m.avg_cost_per_task,
        avg_latency_ms: m.avg_latency_ms,
        p50_latency_ms: m.p50_latency_ms,
        p95_latency_ms: m.p95_latency_ms,
        p99_latency_ms: m.p99_latency_ms,
        avg_quality: m.avg_quality,
        fault_recovery_ms: m.fault_recovery_ms,
        context_efficiency: m.context_efficiency,
    }
}
