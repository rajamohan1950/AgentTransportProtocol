use atp_sim::{BenchMetrics, Scenario, SimHarness, TaskGenerator};
use clap::Parser;
use std::time::Instant;

#[derive(Parser)]
#[command(
    name = "atp-bench",
    about = "AgentNet-Bench: ATP Protocol Benchmark Suite",
    version
)]
struct Cli {
    /// Number of agents in the network.
    #[arg(short, long, default_value = "50")]
    agents: usize,

    /// Number of tasks to simulate.
    #[arg(short, long, default_value = "10000")]
    tasks: usize,

    /// Random seed for reproducibility.
    #[arg(short, long, default_value = "42")]
    seed: u64,

    /// Output format: json, csv, or table.
    #[arg(short, long, default_value = "table")]
    output: String,

    /// Run only a specific scenario.
    #[arg(long)]
    scenario: Option<String>,

    /// Context size (tokens) per task.
    #[arg(long, default_value = "50000")]
    context_size: usize,
}

fn main() {
    let cli = Cli::parse();

    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║           AgentNet-Bench: ATP Protocol Benchmark            ║");
    println!("║         Agent Transport Protocol v0.1.0                     ║");
    println!("║         AlphaForge AI Labs - Rajamohan Jabbala              ║");
    println!("╚══════════════════════════════════════════════════════════════╝");
    println!();
    println!("Configuration:");
    println!("  Agents:       {}", cli.agents);
    println!("  Tasks:        {}", cli.tasks);
    println!("  Seed:         {}", cli.seed);
    println!("  Context size: {} tokens", cli.context_size);
    println!();

    // Build the network
    let start = Instant::now();
    let harness = SimHarness::benchmark(cli.seed);
    println!(
        "Network initialized: {} agents, {} edges ({:.1}ms)",
        harness.network.agent_count(),
        harness.network.edges.len(),
        start.elapsed().as_secs_f64() * 1000.0
    );

    // Generate tasks
    let mut rng = <rand::rngs::StdRng as rand::SeedableRng>::seed_from_u64(cli.seed);
    let generator = TaskGenerator::new().with_context_size(cli.context_size);
    let tasks = generator.generate(cli.tasks, &mut rng);
    println!("Generated {} tasks across 4 categories", tasks.len());
    println!();

    // Define scenarios
    let scenarios: Vec<Scenario> = if let Some(ref s) = cli.scenario {
        vec![match s.as_str() {
            "sequential" => Scenario::Sequential,
            "roundrobin" => Scenario::RoundRobin,
            "atp" => Scenario::FullAtp,
            "nocontext" => Scenario::AtpNoContext,
            "norouting" => Scenario::AtpNoRouting,
            "notrust" => Scenario::AtpNoTrust,
            "nofault" => Scenario::AtpNoFault,
            _ => {
                eprintln!("Unknown scenario: {s}. Options: sequential, roundrobin, atp, nocontext, norouting, notrust, nofault");
                std::process::exit(1);
            }
        }]
    } else {
        vec![
            Scenario::Sequential,
            Scenario::RoundRobin,
            Scenario::FullAtp,
            Scenario::AtpNoContext,
            Scenario::AtpNoRouting,
            Scenario::AtpNoTrust,
            Scenario::AtpNoFault,
        ]
    };

    // Run scenarios
    let mut all_metrics = Vec::new();
    for scenario in &scenarios {
        let start = Instant::now();
        print!("Running {scenario}...");
        let metrics = harness.run_scenario(*scenario, &tasks);
        let elapsed = start.elapsed();
        println!(" done ({:.1}s)", elapsed.as_secs_f64());
        all_metrics.push(metrics);
    }
    println!();

    // Output results
    match cli.output.as_str() {
        "json" => print_json(&all_metrics),
        "csv" => print_csv(&all_metrics),
        _ => print_table(&all_metrics),
    }

    // Print comparison summary
    if all_metrics.len() > 2 {
        print_comparison(&all_metrics);
    }
}

fn print_table(metrics: &[BenchMetrics]) {
    println!("┌─────────────────────┬──────────┬──────────┬──────────┬──────────┬──────────┬──────────┬──────────┐");
    println!("│ Scenario            │ Cost/Task│ Latency  │ Quality  │ Recovery │ Ctx Eff  │ Complete │ Failed   │");
    println!("├─────────────────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────┼──────────┤");

    for m in metrics {
        let recovery = if m.fault_recovery_ms.is_infinite() {
            "inf".to_string()
        } else {
            format!("{:.0}ms", m.fault_recovery_ms)
        };

        println!(
            "│ {:<19} │ ${:<7.4} │ {:<7.0}ms│ {:<8.3} │ {:<8} │ {:<8.2}x│ {:<8} │ {:<8} │",
            m.scenario,
            m.avg_cost_per_task,
            m.avg_latency_ms,
            m.avg_quality,
            recovery,
            1.0 / m.context_efficiency.max(0.001),
            m.tasks_completed,
            m.tasks_failed,
        );
    }

    println!("└─────────────────────┴──────────┴──────────┴──────────┴──────────┴──────────┴──────────┴──────────┘");
}

fn print_comparison(metrics: &[BenchMetrics]) {
    let sequential = metrics.iter().find(|m| m.scenario == "Sequential");
    let atp = metrics.iter().find(|m| m.scenario == "ATP (full)");

    if let (Some(seq), Some(atp)) = (sequential, atp) {
        println!();
        println!("═══════════════════════════════════════════════════════════════");
        println!("  ATP vs Sequential Baseline Comparison");
        println!("═══════════════════════════════════════════════════════════════");

        if seq.avg_cost_per_task > 0.0 {
            let cost_reduction = (1.0 - atp.avg_cost_per_task / seq.avg_cost_per_task) * 100.0;
            println!("  Cost reduction:      {cost_reduction:.1}%");
        }

        if seq.avg_latency_ms > 0.0 {
            let latency_reduction = (1.0 - atp.avg_latency_ms / seq.avg_latency_ms) * 100.0;
            println!("  Latency reduction:   {latency_reduction:.1}%");
        }

        let quality_delta = atp.avg_quality - seq.avg_quality;
        println!("  Quality delta:       {quality_delta:+.3}");

        let ctx_improvement = (1.0 / atp.context_efficiency.max(0.001))
            / (1.0 / seq.context_efficiency.max(0.001));
        println!("  Context efficiency:  {ctx_improvement:.1}x better");

        if atp.fault_recovery_ms.is_finite() && seq.fault_recovery_ms.is_infinite() {
            println!(
                "  Fault recovery:      {:.0}ms (vs pipeline death)",
                atp.fault_recovery_ms
            );
        } else if atp.fault_recovery_ms.is_finite() && seq.fault_recovery_ms.is_finite() {
            let recovery_improvement = seq.fault_recovery_ms / atp.fault_recovery_ms;
            println!("  Fault recovery:      {recovery_improvement:.1}x faster");
        }

        println!("═══════════════════════════════════════════════════════════════");

        // Ablation summary
        println!();
        println!("  Ablation Study:");
        for m in metrics {
            if m.scenario.starts_with("ATP") && m.scenario != "ATP (full)" {
                let cost_delta = if atp.avg_cost_per_task > 0.0 {
                    (m.avg_cost_per_task / atp.avg_cost_per_task - 1.0) * 100.0
                } else {
                    0.0
                };
                let quality_delta = m.avg_quality - atp.avg_quality;
                println!(
                    "    {:<20} cost: {:+.0}%  quality: {:+.3}",
                    m.scenario, cost_delta, quality_delta
                );
            }
        }
        println!();
    }
}

fn print_json(metrics: &[BenchMetrics]) {
    println!("{}", serde_json::to_string_pretty(metrics).unwrap());
}

fn print_csv(metrics: &[BenchMetrics]) {
    println!("scenario,total_tasks,completed,failed,total_cost,avg_cost,avg_latency_ms,p50_ms,p95_ms,p99_ms,avg_quality,recovery_ms,ctx_efficiency,routing_us");
    for m in metrics {
        println!(
            "{},{},{},{},{:.4},{:.4},{:.1},{:.1},{:.1},{:.1},{:.3},{:.1},{:.4},{:.1}",
            m.scenario,
            m.total_tasks,
            m.tasks_completed,
            m.tasks_failed,
            m.total_cost,
            m.avg_cost_per_task,
            m.avg_latency_ms,
            m.p50_latency_ms,
            m.p95_latency_ms,
            m.p99_latency_ms,
            m.avg_quality,
            m.fault_recovery_ms,
            m.context_efficiency,
            m.routing_time_us,
        );
    }
}
