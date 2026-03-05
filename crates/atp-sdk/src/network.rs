use std::sync::OnceLock;
use std::time::Duration;

use atp_routing::{AgentGraph, EconomicRouter};
use atp_sim::{BenchMetrics, Scenario, SimHarness, TaskGenerator};
use atp_types::{Capability, QoSConstraints};

use crate::parse;
use crate::report::BenchReport;
use crate::results::{CompressResult, RouteResult, TrustInfo};

// ── Global lazy network ──────────────────────────────────────────────

static GLOBAL: OnceLock<Network> = OnceLock::new();

/// Get the global auto-initialized network (50 agents, seed 42).
pub(crate) fn global() -> &'static Network {
    GLOBAL.get_or_init(Network::new)
}

// ── Network ──────────────────────────────────────────────────────────

/// A simulated multi-agent network. Everything is handled for you.
///
/// Usually you don't even need to create one — the free functions
/// (`atp_sdk::route`, `atp_sdk::benchmark`, etc.) use a global network.
///
/// Create your own only if you want a custom seed or agent count:
/// ```rust
/// let net = atp_sdk::Network::with_seed(99);
/// let report = net.benchmark(1000);
/// println!("{report}");
/// ```
pub struct Network {
    harness: SimHarness,
    seed: u64,
    agent_count: usize,
}

impl Network {
    /// Create the default benchmark network: 50 agents, seed 42.
    pub fn new() -> Self {
        Self::with_seed(42)
    }

    /// Create a benchmark network with a specific seed for reproducibility.
    pub fn with_seed(seed: u64) -> Self {
        let harness = SimHarness::benchmark(seed);
        let count = harness.network.agent_count();
        Self {
            harness,
            seed,
            agent_count: count,
        }
    }

    /// Number of agents in the network.
    pub fn agents(&self) -> usize {
        self.agent_count
    }

    /// Find the best route for a task type.
    ///
    /// Pass any intuitive string: `"coding"`, `"analysis"`, `"writing"`, `"data"`.
    pub fn route(&self, skill: &str) -> RouteResult {
        self.route_with_quality(skill, 0.3)
    }

    /// Find the best route with a minimum quality constraint.
    pub fn route_with_quality(&self, skill: &str, min_quality: f64) -> RouteResult {
        let task_type = parse::parse(skill);

        // Build a temporary AgentGraph from the sim network's agents
        let mut graph = AgentGraph::new();
        for agent in &self.harness.network.agents {
            let caps: Vec<Capability> = agent
                .capabilities
                .iter()
                .map(|c| c.to_capability())
                .collect();
            graph.add_agent(agent.id, caps, 0.8);
        }
        graph.fully_connect(Duration::from_millis(5));

        let router = EconomicRouter::new(graph);
        let qos = QoSConstraints {
            min_quality,
            max_latency: Duration::from_secs(30),
            max_cost: 10.0,
            min_trust: 0.3,
        };

        match router.find_route(task_type, &qos, None) {
            Ok(route) => RouteResult::from_route(route, task_type),
            Err(e) => RouteResult::failed(task_type, &format!("{e}")),
        }
    }

    /// Run a simulated task and return the result metrics.
    pub fn run(&self, skill: &str, _payload: &[u8]) -> RouteResult {
        let task_type = parse::parse(skill);
        let mut rng = <rand::rngs::StdRng as rand::SeedableRng>::seed_from_u64(self.seed);
        let generator = TaskGenerator::new();
        let tasks = generator.generate(1, &mut rng);
        let metrics = self.harness.run_scenario(Scenario::FullAtp, &tasks);
        RouteResult::from_metrics(task_type, &metrics)
    }

    /// Run the full benchmark with the specified number of tasks.
    ///
    /// Returns a `BenchReport` that prints as a beautiful table.
    pub fn benchmark(&self, task_count: usize) -> BenchReport {
        let mut rng = <rand::rngs::StdRng as rand::SeedableRng>::seed_from_u64(self.seed);
        let generator = TaskGenerator::new().with_context_size(50_000);
        let tasks = generator.generate(task_count, &mut rng);

        let scenarios = [
            Scenario::Sequential,
            Scenario::RoundRobin,
            Scenario::FullAtp,
            Scenario::AtpNoContext,
            Scenario::AtpNoRouting,
            Scenario::AtpNoTrust,
            Scenario::AtpNoFault,
        ];

        let all_metrics: Vec<BenchMetrics> = scenarios
            .iter()
            .map(|s| self.harness.run_scenario(*s, &tasks))
            .collect();

        BenchReport::new(all_metrics, self.agent_count, task_count, self.seed)
    }

    /// Compress context data for a task type.
    pub fn compress(&self, data: &[u8], skill: &str) -> CompressResult {
        let task_type = parse::parse(skill);
        let compressor = atp_context::ContextCompressor::new();
        match compressor.compress_for_task(data, task_type, b"task") {
            Ok(diff) => CompressResult {
                original_size: diff.original_size,
                compressed_size: diff.compressed_size,
                ratio: if diff.compressed_size > 0 {
                    diff.original_size as f64 / diff.compressed_size as f64
                } else {
                    f64::INFINITY
                },
                chunks: diff.chunks.len(),
                confidence: diff.confidence,
            },
            Err(_) => CompressResult {
                original_size: data.len() as u64,
                compressed_size: data.len() as u64,
                ratio: 1.0,
                chunks: 0,
                confidence: 0.0,
            },
        }
    }

    /// Get a trust summary for a task type across the network.
    pub fn trust(&self, skill: &str) -> TrustInfo {
        let task_type = parse::parse(skill);

        // Compute average quality across agents that can handle this task type
        let mut total_quality = 0.0;
        let mut count = 0u32;
        for agent in &self.harness.network.agents {
            for cap in &agent.capabilities {
                if cap.task_type == task_type {
                    total_quality += cap.quality_mean;
                    count += 1;
                }
            }
        }

        if count > 0 {
            TrustInfo::new(total_quality / count as f64, count)
        } else {
            TrustInfo::new(0.5, 0)
        }
    }
}

impl Default for Network {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for Network {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Network({} agents, {} edges, seed={})",
            self.agent_count,
            self.harness.network.edges.len(),
            self.seed
        )
    }
}

impl std::fmt::Debug for Network {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self}")
    }
}
