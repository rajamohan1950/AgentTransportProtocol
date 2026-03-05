# Layer 4: Economic Routing

**Crate:** `atp-routing` | **Tests:** 27

Layer 4 provides **Pareto-optimal multi-objective routing** using modified Bellman-Ford with 5 routing patterns.

---

## Multi-Objective Optimization

Agent routing is fundamentally a multi-objective problem. You want:
- **High quality** (multiplicative across agents)
- **Low latency** (additive across agents + transfer time)
- **Low cost** (additive across agents)

These objectives often conflict. ATP finds the **Pareto frontier** — the set of routes where no metric can be improved without worsening another.

### Objective Functions

```
Quality(R)  = ∏ Q(aᵢ)                      [multiplicative — quality compounds]
Latency(R)  = Σ L(aᵢ) + Σ T(edges)         [additive — includes transfer time]
Cost(R)     = Σ C(aᵢ)                       [additive]
```

### Scalarization via Weight Vectors

The Bellman-Ford algorithm optimizes a single scalar. ATP explores the Pareto frontier by running the algorithm with **10 different weight vectors**:

```
W = { (1,0,0), (0,1,0), (0,0,1),      // Pure objectives
      (0.5,0.5,0), (0.5,0,0.5),        // Pairwise blends
      (0,0.5,0.5), (0.34,0.33,0.33),   // Balanced
      (0.6,0.2,0.2), (0.2,0.6,0.2),    // Heavy on one
      (0.2,0.2,0.6) }

Scalar cost = w_q × (1 - Quality) + w_l × Latency/max_l + w_c × Cost/max_c
```

### Complexity

```
O(k² × |W|)
where:
  k   = number of capability-matched agents (typically 5-20)
  |W| = 10 weight vectors

For 50 agents, k ≈ 12, so ≈ 1,440 iterations → < 1 microsecond
```

## Five Routing Patterns

| # | Pattern | Strategy | Best For | Savings |
|---|---------|----------|----------|---------|
| 1 | **DraftRefine** | Cheap agent drafts, specialist refines | Cost-sensitive tasks | 40-70% |
| 2 | **Cascade** | Try cheapest first, escalate on low confidence | Variable difficulty | 30-50% |
| 3 | **ParallelMerge** | Multiple agents process, merge results | Time-sensitive tasks | Latency |
| 4 | **Ensemble** | Multiple agents vote on result | Quality-critical tasks | Reliability |
| 5 | **Pipeline** | Sequential processing chain | Multi-step workflows | Throughput |

### DraftRefine (Most Common)

```
     Budget Agent (draft)
           │
           ▼
     Quality Check
       ┌───┴───┐
     ≥ 0.8    < 0.8
       │        │
      Done   Specialist Agent (refine)
                │
               Done
```

40-70% cost savings because most tasks are handled by the cheap agent.

### Cascade

```
     Cheapest Agent
           │
      Confidence?
       ┌───┴───┐
     High     Low
       │        │
      Done   Next Agent (more expensive)
                │
           Confidence?
            ┌───┴───┐
          High     Low
            │        │
           Done   Next Agent...
```

30-50% cost savings by escalating only when needed.

### Auto-Selection

ATP automatically selects the best pattern based on task type and QoS constraints:

```rust
pub fn auto_select_pattern(
    task_type: TaskType,
    qos: &QoSConstraints,
) -> RoutingPattern
```

## Agent Graph

Routes are computed over an `AgentGraph` — a directed graph where:
- **Nodes** = agents with capabilities (task type, quality, latency, cost)
- **Edges** = communication links with transfer latency

```rust
pub struct AgentGraph {
    // Methods
    pub fn new() -> Self
    pub fn add_agent(id, capabilities, trust_score) -> usize
    pub fn connect(a, b, transfer_latency)
    pub fn fully_connect(transfer_latency)
    pub fn remove_agent(id)
    pub fn restore_agent(id)
    pub fn node_count() -> usize
}
```

## Usage

```rust
// Simple — prints the best route
atp_sdk::route("coding");
// Output: "Route: draft_refine via 2 agents (q=0.92, $0.0500, 45ms)"

// With minimum quality constraint
let route = atp_sdk::find_route_with("coding", 0.9);
println!("Pattern: {}", route.pattern);
println!("Quality: {:.2}", route.quality);
println!("Cost: ${:.4}", route.cost);
```

## Benchmark Impact

| Scenario | Cost/Task | Quality |
|----------|-----------|---------|
| ATP (full) | $0.0393 | 0.904 |
| ATP w/o Routing | $0.0458 | 0.878 |

Removing routing increases cost by 17% and drops quality by 0.026.

## Next Steps

- [[Layer 5: Fault Tolerance]] — What happens when agents fail
- [[Benchmarks]] — Full routing performance analysis
