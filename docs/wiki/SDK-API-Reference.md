# SDK API Reference

**Crate:** `atp-sdk` | **Tests:** 28 (13 unit + 15 doc-tests)

The SDK provides a dead-simple facade over the entire ATP protocol stack. Two flavors: **verb** functions (print) and **noun** functions (return typed values).

---

## Design Philosophy

- **Zero ceremony**: No structs to create, no config to pass, no setup required
- **Global network**: A 50-agent network is lazily initialized on first use
- **Two flavors**: Verb functions print, noun functions return
- **Display on everything**: Every return type implements `Display`
- **String-based tasks**: Just pass `"coding"` — no enum imports needed

---

## Verb Functions (Fire & Forget)

These functions print results directly to stdout. Perfect for exploration, demos, and quick checks.

### `benchmark()`
```rust
pub fn benchmark()
```
Prints the full 7-scenario benchmark table (50 agents, 10,000 tasks).

**Output:**
```
════════════════════════════════════════════
  ATP Benchmark: 50 agents, 10000 tasks, seed=42
════════════════════════════════════════════
Scenario             Cost/Task  Latency  Quality  ...
──────────────────────────────────────────────────
Sequential            $0.0844    800ms    0.837  ...
ATP (full)            $0.0393    568ms    0.904  ...
...
```

### `route(skill)`
```rust
pub fn route(skill: &str)
```
Prints the best route for the given task type.

**Example:** `atp_sdk::route("coding");`
**Output:** `Route: draft_refine via 2 agents (q=0.92, $0.0500, 45ms)`

### `compress(data, skill)`
```rust
pub fn compress(data: &[u8], skill: &str)
```
Prints context compression statistics.

**Example:** `atp_sdk::compress(b"context data...", "analysis");`
**Output:** `28.3x compression (50000B → 1768B, 3 chunks, confidence=0.85)`

### `sign(message)`
```rust
pub fn sign(message: &[u8])
```
Creates a new agent, signs the message, verifies, and prints everything.

**Example:** `atp_sdk::sign(b"hello");`
**Output:**
```
Agent:     did:key:z6Mk...
Signature: a3f2b1c9...
Verified:  true
```

### `trust(skill)`
```rust
pub fn trust(skill: &str)
```
Prints the average trust score for the given task type.

**Example:** `atp_sdk::trust("coding");`
**Output:** `Trust: 0.87 (n=42)`

---

## Noun Functions (Return Values)

These return typed results for use in production code and pipelines.

### `bench(tasks) -> BenchReport`
```rust
pub fn bench(tasks: usize) -> BenchReport
```
Runs the benchmark and returns a structured report.

```rust
let report = atp_sdk::bench(10_000);
let atp = report.atp().unwrap();
println!("Cost: ${:.4}", atp.avg_cost_per_task);
println!("Quality: {:.3}", atp.avg_quality);

// Access any scenario
let seq = report.scenario("sequential").unwrap();

// Iterate all
for s in report.all() {
    println!("{}: ${:.4}", s.scenario, s.avg_cost_per_task);
}

// Pretty print (uses Display)
println!("{report}");
```

### `find_route(skill) -> RouteResult`
```rust
pub fn find_route(skill: &str) -> RouteResult
pub fn find_route_with(skill: &str, min_quality: f64) -> RouteResult
```

```rust
let route = atp_sdk::find_route("coding");
println!("Pattern: {}", route.pattern);   // "draft_refine"
println!("Agents: {}", route.agents);     // 2
println!("Quality: {:.2}", route.quality); // 0.92
println!("Cost: ${:.4}", route.cost);      // 0.0500
println!("Latency: {}ms", route.latency_ms); // 45

// With minimum quality constraint
let route = atp_sdk::find_route_with("coding", 0.95);
```

### `shrink(data, skill) -> CompressResult`
```rust
pub fn shrink(data: &[u8], skill: &str) -> CompressResult
```

```rust
let comp = atp_sdk::shrink(b"context data...", "analysis");
println!("Ratio: {}x", comp.ratio);           // 28.3
println!("Original: {}B", comp.original_size);  // 50000
println!("Compressed: {}B", comp.compressed_size); // 1768
println!("Chunks: {}", comp.chunks);            // 3
println!("Confidence: {:.2}", comp.confidence);  // 0.85
```

### `agent() -> Agent`
```rust
pub fn agent() -> Agent
```

```rust
let a = atp_sdk::agent();
println!("DID: {}", a.did());                // "did:key:z6Mk..."
println!("Public key: {}", a.public_key_hex()); // "a3f2b1c9..."

let sig = a.sign(b"hello");
assert!(a.verify(b"hello", &sig));
assert!(!a.verify(b"tampered", &sig));

// Display
println!("{a}");    // "Agent(did:key:z6Mk...abc)"
println!("{sig}");  // "Sig(a3f2b1c9...)"
```

### `trust_score(skill) -> TrustInfo`
```rust
pub fn trust_score(skill: &str) -> TrustInfo
```

```rust
let trust = atp_sdk::trust_score("coding");
println!("Score: {:.2}", trust.score);   // 0.87
println!("Samples: {}", trust.samples);  // 42
```

---

## Skill Aliases

All skill parameters accept case-insensitive strings with many aliases:

| Canonical | Aliases |
|-----------|---------|
| `"coding"` | `"code"`, `"codegen"`, `"code_generation"`, `"cg"` |
| `"analysis"` | `"analyze"`, `"analyse"` |
| `"writing"` | `"creative"`, `"creative_writing"`, `"cw"` |
| `"data"` | `"processing"`, `"data_processing"`, `"dp"` |

---

## Advanced: Direct Network Access

For advanced use cases, you can create your own network:

```rust
use atp_sdk::Network;

let net = Network::new();              // 50 agents, seed 42
let net = Network::with_seed(123);     // Custom seed

let route = net.route("coding");
let comp = net.compress(b"data", "coding");
let trust = net.trust("analysis");
let report = net.benchmark(10_000);

println!("Agents: {}", net.agents());  // 50
println!("{net}");  // "Network(50 agents, 247 edges, seed=42)"
```

## Next Steps

- [[Python SDK]] — Same API in Python
- [[Getting Started]] — Build your first ATP program
