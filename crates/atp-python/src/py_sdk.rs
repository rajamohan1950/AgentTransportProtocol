use pyo3::prelude::*;
use atp_sim::{SimHarness, Scenario, TaskGenerator};
use atp_context::ContextCompressor;
use atp_identity::{DidGenerator, KeyPair};
use atp_routing::{AgentGraph, EconomicRouter};
use atp_types::{Capability, QoSConstraints, TaskType};
use std::time::Duration;

/// Print the full benchmark table. Zero setup.
///
///     >>> import atp
///     >>> atp.benchmark()
#[pyfunction]
#[pyo3(name = "benchmark")]
pub fn py_benchmark() {
    let harness = SimHarness::benchmark(42);
    let mut rng = <rand::rngs::StdRng as rand::SeedableRng>::seed_from_u64(42);
    let generator = TaskGenerator::new().with_context_size(50_000);
    let tasks = generator.generate(10_000, &mut rng);

    let scenarios = [
        Scenario::Sequential,
        Scenario::RoundRobin,
        Scenario::FullAtp,
        Scenario::AtpNoContext,
        Scenario::AtpNoRouting,
        Scenario::AtpNoTrust,
        Scenario::AtpNoFault,
    ];

    println!("════════════════════════════════════════════════════════════════════");
    println!("  ATP Benchmark: 50 agents, 10000 tasks, seed=42");
    println!("════════════════════════════════════════════════════════════════════");
    println!();
    println!(
        "{:<20} {:>9} {:>8} {:>8} {:>9} {:>6} {:>6}",
        "Scenario", "Cost/Task", "Latency", "Quality", "Recovery", "Ctx", "Failed"
    );
    println!("{}", "─".repeat(68));

    for scenario in &scenarios {
        let m = harness.run_scenario(*scenario, &tasks);
        let recovery = if m.fault_recovery_ms.is_infinite() {
            "inf".to_string()
        } else {
            format!("{:.0}ms", m.fault_recovery_ms)
        };
        let ctx = 1.0 / m.context_efficiency.max(0.001);
        println!(
            "{:<20} ${:>7.4} {:>6.0}ms {:>7.3} {:>9} {:>5.1}x {:>6}",
            m.scenario, m.avg_cost_per_task, m.avg_latency_ms, m.avg_quality,
            recovery, ctx, m.tasks_failed,
        );
    }
    println!("{}", "─".repeat(68));
}

/// Print the best route for a task type.
///
///     >>> atp.route("coding")
#[pyfunction]
#[pyo3(name = "route")]
pub fn py_route(skill: &str) {
    let task_type = parse_skill(skill);
    let harness = SimHarness::benchmark(42);
    let mut graph = AgentGraph::new();
    for agent in &harness.network.agents {
        let caps: Vec<Capability> = agent.capabilities.iter().map(|c| c.to_capability()).collect();
        graph.add_agent(agent.id, caps, 0.8);
    }
    graph.fully_connect(Duration::from_millis(5));
    let router = EconomicRouter::new(graph);
    let qos = QoSConstraints {
        min_quality: 0.3,
        max_latency: Duration::from_secs(30),
        max_cost: 10.0,
        min_trust: 0.3,
    };
    match router.find_route(task_type, &qos, None) {
        Ok(route) => println!(
            "Route: {} via {} agent{} (q={:.2}, ${:.4}, {}ms)",
            route.pattern,
            route.agents.len(),
            if route.agents.len() == 1 { "" } else { "s" },
            route.metrics.quality,
            route.metrics.cost,
            route.metrics.latency.as_millis(),
        ),
        Err(e) => println!("Route: FAILED — {e}"),
    }
}

/// Print context compression results.
///
///     >>> atp.compress(b"big data...", "coding")
#[pyfunction]
#[pyo3(name = "compress")]
pub fn py_compress(data: &[u8], skill: &str) {
    let task_type = parse_skill(skill);
    let compressor = ContextCompressor::new();
    match compressor.compress_for_task(data, task_type, b"task") {
        Ok(diff) => {
            let ratio = if diff.compressed_size > 0 {
                diff.original_size as f64 / diff.compressed_size as f64
            } else {
                f64::INFINITY
            };
            println!(
                "{:.1}x compression ({}B → {}B, {} chunks)",
                ratio, diff.original_size, diff.compressed_size, diff.chunks.len(),
            );
        }
        Err(e) => println!("Compression failed: {e}"),
    }
}

/// Create an agent, sign the message, and print it.
///
///     >>> atp.sign(b"hello")
#[pyfunction]
#[pyo3(name = "sign")]
pub fn py_sign(message: &[u8]) {
    let keypair = KeyPair::generate().expect("key generation failed");
    let did = DidGenerator::generate_did(&keypair.public_key_bytes())
        .expect("DID generation failed");
    let uri = did.to_uri();
    let sig = keypair.sign(message);
    let verified = keypair.verify(message, &sig).is_ok();
    let hex: String = sig.to_bytes().iter().take(8).map(|b| format!("{b:02x}")).collect();

    let short_uri = if uri.len() > 30 {
        format!("{}...{}", &uri[..20], &uri[uri.len() - 6..])
    } else {
        uri
    };
    println!("Agent({short_uri})");
    println!("  Signed: Sig({hex}...)");
    println!("  Verified: {verified}");
}

/// Print trust info for a task type.
///
///     >>> atp.trust("coding")
#[pyfunction]
#[pyo3(name = "trust")]
pub fn py_trust(skill: &str) {
    let task_type = parse_skill(skill);
    let harness = SimHarness::benchmark(42);
    let mut total = 0.0;
    let mut count = 0u32;
    for agent in &harness.network.agents {
        for cap in &agent.capabilities {
            if cap.task_type == task_type {
                total += cap.quality_mean;
                count += 1;
            }
        }
    }
    let score = if count > 0 { total / count as f64 } else { 0.5 };
    println!("Trust: {score:.2} (n={count})");
}

fn parse_skill(s: &str) -> TaskType {
    match s.to_lowercase().trim() {
        "coding" | "code" | "codegen" | "code_generation" | "cg" => TaskType::CodeGeneration,
        "analysis" | "analyze" | "analyse" => TaskType::Analysis,
        "writing" | "creative" | "creative_writing" | "cw" => TaskType::CreativeWriting,
        "data" | "processing" | "data_processing" | "dp" => TaskType::DataProcessing,
        other => panic!("Unknown task type: '{other}'. Try: coding, analysis, writing, or data"),
    }
}
