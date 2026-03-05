# Benchmarks

**Crate:** `atp-bench` | **Framework:** AgentNet-Bench

Reproducible benchmarks with 50 agents, 10,000 tasks, seed=42. All numbers deterministic.

---

## Full Results

```
════════════════════════════════════════════════════════════════════
  ATP Benchmark: 50 agents, 10000 tasks, seed=42
════════════════════════════════════════════════════════════════════

Scenario             Cost/Task  Latency  Quality  Recovery    Ctx  Failed
────────────────────────────────────────────────────────────────────────
Sequential            $0.0844    800ms    0.837       inf   1.0x       0
Round-Robin           $0.0712    720ms    0.856       inf   1.0x       0
ATP (full)            $0.0393    568ms    0.904       0ms  28.0x       0
ATP w/o SCD           $0.0627    612ms    0.891       0ms   1.0x       0
ATP w/o Routing       $0.0458    645ms    0.878       0ms  28.0x       0
ATP w/o Trust         $0.0451    634ms    0.892       0ms  28.0x       0
ATP w/o Fault         $0.0397    580ms    0.902       inf  28.0x       2
────────────────────────────────────────────────────────────────────────

ATP vs Sequential:
  Cost:    -53.4%
  Latency: -29.0%
  Quality: +0.067
```

## Scenarios Explained

### 1. Sequential (Baseline)
Tasks assigned to a single agent in order. No optimization. This is the worst case.

### 2. Round-Robin
Tasks distributed evenly across agents. No intelligence — just rotation.

### 3. ATP (Full)
All 5 layers active: identity, handshake, SCD compression, economic routing, fault tolerance.

### 4-7. Ablation Studies
Each removes exactly one layer to measure its isolated contribution:

| Removed Layer | Impact |
|---------------|--------|
| **SCD (L3)** | Cost +59%, compression drops to 1.0x |
| **Routing (L4)** | Cost +17%, quality -0.026 |
| **Trust (L1)** | Cost +15%, quality -0.012 |
| **Fault (L5)** | 2 failures, infinite recovery time |

## Key Takeaways

### 1. Every Layer Contributes
No layer is redundant. Removing any single layer measurably degrades results.

### 2. SCD Is the Biggest Cost Saver
Context compression (Layer 3) provides the largest single cost reduction — 59% more expensive without it. This makes sense: sending 28x less context means 28x lower token costs.

### 3. Routing Drives Quality
Economic routing (Layer 4) provides the biggest quality improvement. By selecting the right agents for the right tasks, quality jumps from 0.878 to 0.904.

### 4. Fault Tolerance Is Binary
Without fault tolerance (Layer 5), tasks can fail permanently. With it, zero failures. There's no middle ground.

### 5. Trust Prevents Bad Assignments
Trust scoring (Layer 1) prevents low-quality agents from getting high-stakes tasks, improving both cost and quality.

## CLI Options

```bash
# Default run
cargo run --release -p atp-bench

# Custom parameters
cargo run --release -p atp-bench -- \
  --agents 100 \
  --tasks 50000 \
  --seed 123

# Output formats
cargo run --release -p atp-bench -- --output json
cargo run --release -p atp-bench -- --output csv
cargo run --release -p atp-bench -- --output table  # default

# Single scenario
cargo run --release -p atp-bench -- --scenario atp
cargo run --release -p atp-bench -- --scenario sequential
cargo run --release -p atp-bench -- --scenario nofault

# Custom context size
cargo run --release -p atp-bench -- --context_size 100000
```

## Metrics Collected

For each scenario, AgentNet-Bench tracks:

| Metric | Description |
|--------|-------------|
| `total_tasks` | Tasks submitted |
| `tasks_completed` | Successfully completed |
| `tasks_failed` | Failed permanently |
| `total_cost` | Sum of all task costs (USD) |
| `avg_cost_per_task` | Mean cost per task |
| `avg_latency_ms` | Mean latency |
| `p50_latency_ms` | Median latency |
| `p95_latency_ms` | 95th percentile latency |
| `p99_latency_ms` | 99th percentile latency |
| `avg_quality` | Mean quality score (0-1) |
| `fault_recovery_ms` | Mean recovery time |
| `context_efficiency` | Compression ratio |
| `routing_time_us` | Time spent in routing |

## Reproducing Results

```bash
# Exact reproduction (deterministic)
cargo run --release -p atp-bench -- --agents 50 --tasks 10000 --seed 42

# Same results every time due to seeded RNG
```

## Next Steps

- [[Architecture Overview]] — Understand why each layer matters
- [[Layer 3: Context Compression]] — The biggest cost driver
- [[Layer 4: Economic Routing]] — The quality driver
