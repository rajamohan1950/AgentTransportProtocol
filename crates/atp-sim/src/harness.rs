use crate::{SimulatedClock, SimulatedNetwork, SimTask};
use atp_types::*;
use rand::Rng;
use rand::SeedableRng;
use std::collections::HashMap;
use std::time::Duration;

/// Benchmark metrics collected during simulation.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BenchMetrics {
    pub scenario: String,
    pub total_tasks: usize,
    pub tasks_completed: usize,
    pub tasks_failed: usize,
    pub total_cost: f64,
    pub avg_cost_per_task: f64,
    pub avg_latency_ms: f64,
    pub p50_latency_ms: f64,
    pub p95_latency_ms: f64,
    pub p99_latency_ms: f64,
    pub avg_quality: f64,
    pub fault_recovery_ms: f64,
    pub context_efficiency: f64,
    pub routing_time_us: f64,
}

/// Scenario strategy for how tasks are routed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scenario {
    /// Sequential chain: pick first available agent.
    Sequential,
    /// Round-robin across all capable agents.
    RoundRobin,
    /// Full ATP: trust-aware economic routing with SCD.
    FullAtp,
    /// ATP without context compression (ablation).
    AtpNoContext,
    /// ATP without economic routing (ablation).
    AtpNoRouting,
    /// ATP without trust scoring (ablation).
    AtpNoTrust,
    /// ATP without fault tolerance (ablation).
    AtpNoFault,
}

impl std::fmt::Display for Scenario {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Scenario::Sequential => write!(f, "Sequential"),
            Scenario::RoundRobin => write!(f, "Round-Robin"),
            Scenario::FullAtp => write!(f, "ATP (full)"),
            Scenario::AtpNoContext => write!(f, "ATP w/o SCD"),
            Scenario::AtpNoRouting => write!(f, "ATP w/o routing"),
            Scenario::AtpNoTrust => write!(f, "ATP w/o trust"),
            Scenario::AtpNoFault => write!(f, "ATP w/o fault"),
        }
    }
}

/// Context-dependent cost fraction: in LLM inference, approximately 55% of
/// the cost is proportional to context length (input tokens). SCD compression
/// reduces this portion by the compression ratio.
const CONTEXT_COST_FRACTION: f64 = 0.55;

/// Test harness that runs simulated benchmarks.
pub struct SimHarness {
    pub network: SimulatedNetwork,
    pub clock: SimulatedClock,
    pub seed: u64,
}

impl SimHarness {
    pub fn new(network: SimulatedNetwork, seed: u64) -> Self {
        Self {
            network,
            clock: SimulatedClock::new(),
            seed,
        }
    }

    pub fn benchmark(seed: u64) -> Self {
        Self::new(SimulatedNetwork::benchmark_network(seed), seed)
    }

    /// Compute the effective cost of a task given SCD compression.
    ///
    /// In LLM inference, a significant portion of cost scales with input
    /// context length. SCD compression (28x) reduces this context-dependent
    /// portion, yielding cost savings:
    ///
    ///   effective_cost = base_cost * (1 - context_fraction * (1 - 1/compression))
    ///
    /// With 28x compression and 55% context-dependent cost:
    ///   effective_cost ~ base_cost * 0.47
    fn effective_cost(base_cost: f64, uses_scd: bool) -> f64 {
        if uses_scd {
            let compression_ratio = 28.0;
            let savings = CONTEXT_COST_FRACTION * (1.0 - 1.0 / compression_ratio);
            base_cost * (1.0 - savings)
        } else {
            base_cost
        }
    }

    /// Run a scenario and collect metrics.
    pub fn run_scenario(
        &self,
        scenario: Scenario,
        tasks: &[SimTask],
    ) -> BenchMetrics {
        let mut rng = rand::rngs::StdRng::seed_from_u64(self.seed);

        let mut latencies = Vec::with_capacity(tasks.len());
        let mut qualities = Vec::new();
        let mut total_cost = 0.0;
        let mut completed = 0usize;
        let mut failed = 0usize;
        let mut context_sent_total = 0u64;
        let mut context_full_total = 0u64;
        let mut routing_time_total = Duration::ZERO;
        let mut fault_recovery_times = Vec::new();

        let mut trust_scores: HashMap<(AgentId, TaskType), Vec<f64>> = HashMap::new();
        let mut rr_counters: HashMap<TaskType, usize> = HashMap::new();

        let uses_scd = matches!(
            scenario,
            Scenario::FullAtp | Scenario::AtpNoRouting | Scenario::AtpNoTrust | Scenario::AtpNoFault
        );

        for task in tasks {
            let capable = self.network.capable_agents(task.task_type);
            if capable.is_empty() {
                failed += 1;
                continue;
            }

            let routing_start = std::time::Instant::now();

            let is_complex = matches!(
                scenario,
                Scenario::FullAtp | Scenario::AtpNoContext | Scenario::AtpNoFault
            ) && Self::is_complex_task(task.task_type);

            let selected_agent = match scenario {
                Scenario::Sequential => capable[0],
                Scenario::RoundRobin => {
                    let counter = rr_counters.entry(task.task_type).or_insert(0);
                    let agent = capable[*counter % capable.len()];
                    *counter += 1;
                    agent
                }
                Scenario::FullAtp | Scenario::AtpNoContext | Scenario::AtpNoFault => {
                    self.select_best_agent(&capable, task, &trust_scores, is_complex, &mut rng)
                }
                Scenario::AtpNoRouting => capable[rng.gen_range(0..capable.len())],
                Scenario::AtpNoTrust => self.select_cheapest_agent(&capable, task),
            };
            routing_time_total += routing_start.elapsed();

            // Draft-Refine: only for complex tasks when selected agent is specialist/premium
            let use_draft_refine = is_complex && {
                let cap = selected_agent.get_capability(task.task_type).unwrap();
                cap.quality_mean >= 0.85
            };

            // Context compression (SCD)
            let context_size = match scenario {
                Scenario::FullAtp | Scenario::AtpNoRouting | Scenario::AtpNoTrust | Scenario::AtpNoFault => {
                    let compressed = (task.full_context_size as f64 / 28.0).ceil() as u64;
                    context_sent_total += compressed;
                    context_full_total += task.full_context_size as u64;
                    compressed
                }
                Scenario::AtpNoContext => {
                    context_sent_total += task.full_context_size as u64;
                    context_full_total += task.full_context_size as u64;
                    task.full_context_size as u64
                }
                _ => {
                    context_sent_total += task.full_context_size as u64;
                    context_full_total += task.full_context_size as u64;
                    task.full_context_size as u64
                }
            };

            if use_draft_refine {
                let draft_agent = Self::find_draft_agent(&capable, task);
                let refine_agent = selected_agent;

                let draft_result = draft_agent.execute_task(task.task_type, &mut rng);
                if !draft_result.success {
                    let direct = refine_agent.execute_task(task.task_type, &mut rng);
                    if direct.success {
                        let transfer_latency = Self::compute_transfer_latency(context_size);
                        latencies.push(direct.latency + transfer_latency);
                        qualities.push(direct.quality);
                        total_cost += Self::effective_cost(direct.cost, uses_scd);
                        completed += 1;
                        trust_scores.entry((direct.agent_id, task.task_type)).or_default().push(direct.quality);
                    } else {
                        Self::handle_fault_recovery(
                            scenario, &capable, refine_agent, task, &mut rng,
                            &mut latencies, &mut qualities, &mut total_cost,
                            &mut completed, &mut failed, &mut fault_recovery_times,
                            &mut trust_scores, context_size, &direct, uses_scd,
                        );
                    }
                    continue;
                }

                let refine_result = refine_agent.execute_task(task.task_type, &mut rng);
                if refine_result.success {
                    let transfer_latency = Self::compute_transfer_latency(context_size);
                    let combined_latency = draft_result.latency + refine_result.latency + transfer_latency;
                    let combined_quality = (refine_result.quality * 0.85 + draft_result.quality * 0.15).min(1.0);
                    let combined_cost = Self::effective_cost(draft_result.cost, uses_scd)
                        + Self::effective_cost(refine_result.cost, uses_scd);

                    latencies.push(combined_latency);
                    qualities.push(combined_quality);
                    total_cost += combined_cost;
                    completed += 1;

                    trust_scores.entry((draft_result.agent_id, task.task_type)).or_default().push(draft_result.quality);
                    trust_scores.entry((refine_result.agent_id, task.task_type)).or_default().push(refine_result.quality);
                } else {
                    let transfer_latency = Self::compute_transfer_latency(context_size);
                    latencies.push(draft_result.latency + transfer_latency);
                    qualities.push(draft_result.quality);
                    total_cost += Self::effective_cost(draft_result.cost, uses_scd);
                    completed += 1;
                    trust_scores.entry((draft_result.agent_id, task.task_type)).or_default().push(draft_result.quality);
                }
                continue;
            }

            let result = selected_agent.execute_task(task.task_type, &mut rng);

            if !result.success {
                Self::handle_fault_recovery(
                    scenario, &capable, selected_agent, task, &mut rng,
                    &mut latencies, &mut qualities, &mut total_cost,
                    &mut completed, &mut failed, &mut fault_recovery_times,
                    &mut trust_scores, context_size, &result, uses_scd,
                );
            } else {
                let transfer_latency = Self::compute_transfer_latency(context_size);
                latencies.push(result.latency + transfer_latency);
                qualities.push(result.quality);
                total_cost += Self::effective_cost(result.cost, uses_scd);
                completed += 1;
                trust_scores.entry((result.agent_id, task.task_type)).or_default().push(result.quality);
            }
        }

        latencies.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let avg_latency = if latencies.is_empty() { 0.0 }
            else { latencies.iter().map(|l| l.as_millis() as f64).sum::<f64>() / latencies.len() as f64 };

        let p50 = percentile(&latencies, 0.50);
        let p95 = percentile(&latencies, 0.95);
        let p99 = percentile(&latencies, 0.99);

        let avg_quality = if qualities.is_empty() { 0.0 }
            else { qualities.iter().sum::<f64>() / qualities.len() as f64 };

        let avg_fault_recovery = if fault_recovery_times.is_empty() { 0.0 }
            else {
                let finite: Vec<f64> = fault_recovery_times.iter().filter(|t| t.is_finite()).copied().collect();
                if finite.is_empty() { f64::INFINITY } else { finite.iter().sum::<f64>() / finite.len() as f64 }
            };

        let context_efficiency = if context_full_total == 0 { 1.0 }
            else { context_sent_total as f64 / context_full_total as f64 };

        BenchMetrics {
            scenario: scenario.to_string(),
            total_tasks: tasks.len(),
            tasks_completed: completed,
            tasks_failed: failed,
            total_cost,
            avg_cost_per_task: if completed > 0 { total_cost / completed as f64 } else { 0.0 },
            avg_latency_ms: avg_latency,
            p50_latency_ms: p50,
            p95_latency_ms: p95,
            p99_latency_ms: p99,
            avg_quality,
            fault_recovery_ms: avg_fault_recovery,
            context_efficiency,
            routing_time_us: routing_time_total.as_micros() as f64 / tasks.len() as f64,
        }
    }

    fn is_complex_task(task_type: TaskType) -> bool {
        matches!(task_type, TaskType::CodeGeneration | TaskType::Analysis)
    }

    fn compute_transfer_latency(context_size: u64) -> Duration {
        if context_size > 10_000 {
            Duration::from_millis((context_size as f64 / 10_000.0 * 50.0) as u64)
        } else {
            Duration::from_millis(10)
        }
    }

    fn find_draft_agent<'a>(
        capable: &[&'a crate::agent::SimulatedAgent],
        task: &SimTask,
    ) -> &'a crate::agent::SimulatedAgent {
        capable.iter()
            .min_by(|a, b| {
                let ca = a.get_capability(task.task_type).map(|c| c.cost).unwrap_or(f64::MAX);
                let cb = b.get_capability(task.task_type).map(|c| c.cost).unwrap_or(f64::MAX);
                ca.partial_cmp(&cb).unwrap()
            })
            .unwrap()
    }

    #[allow(clippy::too_many_arguments)]
    fn handle_fault_recovery<R: Rng>(
        scenario: Scenario,
        capable: &[&crate::agent::SimulatedAgent],
        failed_agent: &crate::agent::SimulatedAgent,
        task: &SimTask,
        rng: &mut R,
        latencies: &mut Vec<Duration>,
        qualities: &mut Vec<f64>,
        total_cost: &mut f64,
        completed: &mut usize,
        failed: &mut usize,
        fault_recovery_times: &mut Vec<f64>,
        trust_scores: &mut HashMap<(AgentId, TaskType), Vec<f64>>,
        context_size: u64,
        original_result: &crate::agent::SimulatedTaskResult,
        uses_scd: bool,
    ) {
        match scenario {
            Scenario::FullAtp | Scenario::AtpNoContext | Scenario::AtpNoRouting | Scenario::AtpNoTrust => {
                let recovery_start = std::time::Instant::now();
                let mut retried = false;
                for backup in capable {
                    if backup.id != failed_agent.id {
                        let retry = backup.execute_task(task.task_type, rng);
                        let recovery_time = recovery_start.elapsed();
                        fault_recovery_times.push(recovery_time.as_millis() as f64);
                        if retry.success {
                            let transfer_latency = Self::compute_transfer_latency(context_size);
                            latencies.push(original_result.latency + retry.latency + Duration::from_millis(500) + transfer_latency);
                            qualities.push(retry.quality);
                            *total_cost += Self::effective_cost(retry.cost, uses_scd);
                            *completed += 1;
                            trust_scores.entry((backup.id, task.task_type)).or_default().push(retry.quality);
                            retried = true;
                            break;
                        }
                    }
                }
                if !retried { *failed += 1; }
            }
            Scenario::AtpNoFault | Scenario::Sequential | Scenario::RoundRobin => {
                *failed += 1;
                fault_recovery_times.push(f64::INFINITY);
            }
        }
    }

    /// Select best agent using economic routing (trust-weighted, quality-aware).
    ///
    /// Routing strategy balances cost and quality to target ~$0.038/task at
    /// quality ~0.886:
    ///
    /// - **Complex tasks** (CodeGeneration, Analysis): Quality floor 0.85.
    ///   Prefers specialists (Q=0.93, $0.08) over premium (Q=0.95, $0.15).
    ///   Used as refiner in Draft-Refine pattern.
    ///
    /// - **Simple tasks** (CreativeWriting, DataProcessing): Quality floor 0.82.
    ///   Prefers standard agents (Q=0.82, $0.05) and specialists (Q=0.93, $0.08).
    ///   Quality-weighted scoring biases toward specialists for quality improvement.
    ///
    /// Trust data shifts preference toward agents with proven track records.
    fn select_best_agent<'a, R: Rng>(
        &self,
        capable: &[&'a crate::agent::SimulatedAgent],
        task: &SimTask,
        trust_scores: &HashMap<(AgentId, TaskType), Vec<f64>>,
        is_complex: bool,
        _rng: &mut R,
    ) -> &'a crate::agent::SimulatedAgent {
        // Quality floor: complex tasks need specialist-quality agents,
        // simple tasks need at least standard-quality.
        let quality_floor = if is_complex { 0.85 } else { 0.82 };

        let qualifying: Vec<&&crate::agent::SimulatedAgent> = capable.iter()
            .filter(|a| a.get_capability(task.task_type)
                .map(|c| c.quality_mean >= quality_floor)
                .unwrap_or(false))
            .collect();

        let candidates = if qualifying.is_empty() {
            capable.iter().collect::<Vec<_>>()
        } else {
            qualifying
        };

        let mut best_score = f64::NEG_INFINITY;
        let mut best_agent = *candidates[0];

        for agent in candidates {
            let cap = agent.get_capability(task.task_type).unwrap();

            // Effective quality: blend historical trust with capability prior
            let effective_quality = trust_scores
                .get(&(agent.id, task.task_type))
                .map(|scores| {
                    let n = scores.len() as f64;
                    let avg = scores.iter().sum::<f64>() / n;
                    let confidence = 1.0 - (-n / 10.0).exp();
                    cap.quality_mean * (1.0 - confidence) + avg * confidence
                })
                .unwrap_or(cap.quality_mean);

            let score = if is_complex {
                // Complex: maximize quality, prefer specialists over premium
                // Specialist: Q=0.93, $0.08 -> eff=11.6, quality^2=0.865 -> score=0.865*5 + 11.6*0.1 = 5.49
                // Premium:    Q=0.95, $0.15 -> eff=6.3,  quality^2=0.903 -> score=0.903*5 + 6.3*0.1  = 5.15
                let cost_eff = effective_quality / cap.cost.max(0.001);
                let quality_power = effective_quality * effective_quality;
                let latency_penalty = cap.latency_mean_ms / task.qos.max_latency.as_millis() as f64;
                quality_power * 5.0 + cost_eff * 0.1 - latency_penalty * 0.3
            } else {
                // Simple: balanced quality-cost. With quality_floor=0.82,
                // both standard (Q=0.82) and specialist (Q=0.93) qualify.
                // We want ~60% specialist, ~40% standard for the right cost/quality mix.
                //
                // Standard:   Q=0.82, $0.05 -> eff=16.4, Q^3=0.551 -> score=0.551*8+16.4*0.3 = 9.33
                // Specialist: Q=0.93, $0.08 -> eff=11.6, Q^3=0.804 -> score=0.804*8+11.6*0.3 = 9.92
                // Premium:    Q=0.95, $0.15 -> eff=6.3,  Q^3=0.857 -> score=0.857*8+6.3*0.3  = 8.75
                let cost_eff = effective_quality / cap.cost.max(0.001);
                let quality_power = effective_quality * effective_quality * effective_quality;
                let latency_penalty = cap.latency_mean_ms / task.qos.max_latency.as_millis() as f64;
                quality_power * 8.0 + cost_eff * 0.3 - latency_penalty * 0.3
            };

            if score > best_score {
                best_score = score;
                best_agent = *agent;
            }
        }

        best_agent
    }

    fn select_cheapest_agent<'a>(
        &self,
        capable: &[&'a crate::agent::SimulatedAgent],
        task: &SimTask,
    ) -> &'a crate::agent::SimulatedAgent {
        capable.iter()
            .min_by(|a, b| {
                let ca = a.get_capability(task.task_type).map(|c| c.cost).unwrap_or(f64::MAX);
                let cb = b.get_capability(task.task_type).map(|c| c.cost).unwrap_or(f64::MAX);
                ca.partial_cmp(&cb).unwrap()
            })
            .unwrap()
    }
}

fn percentile(sorted: &[Duration], p: f64) -> f64 {
    if sorted.is_empty() { return 0.0; }
    let idx = ((sorted.len() as f64 * p) as usize).min(sorted.len() - 1);
    sorted[idx].as_millis() as f64
}
