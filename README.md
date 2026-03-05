<p align="center">
  <img src="https://img.shields.io/badge/Rust-000000?style=for-the-badge&logo=rust&logoColor=white" alt="Rust">
  <img src="https://img.shields.io/badge/gRPC-244c5a?style=for-the-badge&logo=google&logoColor=white" alt="gRPC">
  <img src="https://img.shields.io/badge/Protobuf-4285F4?style=for-the-badge&logo=google&logoColor=white" alt="Protobuf">
  <img src="https://img.shields.io/badge/License-MIT%2FApache--2.0-blue?style=for-the-badge" alt="License">
</p>

<h1 align="center">Agent Transport Protocol (ATP)</h1>

<h3 align="center"><em>The TCP/IP of AI Agents</em></h3>

<p align="center">
  Five-layer protocol stack for trust-aware, economically-optimal multi-agent networking.<br/>
  One line of code. Zero config. Production-grade Rust.
</p>

<p align="center">
  <a href="#-quick-start">Quick Start</a> &bull;
  <a href="#-architecture">Architecture</a> &bull;
  <a href="#-benchmarks">Benchmarks</a> &bull;
  <a href="#-sdk-api">SDK API</a> &bull;
  <a href="#-why-atp">Why ATP?</a> &bull;
  <a href="https://github.com/rajamohan1950/AgentTransportProtocol/wiki">Wiki</a>
</p>

---

## Headline Numbers

| Metric | Value |
|--------|-------|
| **Cost Reduction** | **-53.4%** vs sequential |
| **Context Compression** | **28x** via Semantic Context Differentials |
| **Task Failures** | **0** across 10,000 tasks |
| **Quality Score** | **0.904** (+8% over baselines) |
| **Latency Reduction** | **-29.3%** vs sequential |
| **Routing Decisions** | **< 1 microsecond** |
| **Tests** | **280 passing** &bull; zero failures |
| **Lines of Rust** | **~37,000** across 75 files |

---

## Quick Start

### Rust (3 lines)

```toml
# Cargo.toml
[dependencies]
atp-sdk = { git = "https://github.com/rajamohan1950/AgentTransportProtocol" }
```

```rust
fn main() {
    atp_sdk::benchmark();                    // Full 7-scenario benchmark table
    atp_sdk::route("coding");                // Best route for coding tasks
    atp_sdk::compress(b"context...", "coding"); // 28x compression
}
```

```bash
cargo run
```

**That's it.** No config. No setup. No structs to create. Just call the function.

### Python

```bash
pip install maturin
cd crates/atp-python && maturin develop --release
```

```python
import atp

atp.benchmark()           # Full 7-scenario table
atp.route("coding")       # Best agent route
atp.compress(data, "coding")  # 28x compression
atp.sign(b"hello")        # Ed25519 identity + signature
atp.trust("coding")       # Network trust score
```

### CLI Benchmark

```bash
cargo run --release -p atp-bench -- --agents 50 --tasks 10000 --seed 42
```

---

## Architecture

ATP is a five-layer protocol stack. Each layer is independent, composable, and has its own crate:

```
┌─────────────────────────────────────────────────────────┐
│  L5  Fault Tolerance     Circuit breaker, heartbeat,    │
│                          poison pill detection           │
├─────────────────────────────────────────────────────────┤
│  L4  Economic Routing    Bellman-Ford, 5 patterns,      │
│                          Pareto-optimal multi-objective  │
├─────────────────────────────────────────────────────────┤
│  L3  Context (SCD)       28x semantic compression,      │
│                          cosine similarity, MSC extract  │
├─────────────────────────────────────────────────────────┤
│  L2  Handshake           3-phase SYN/SYN-ACK/ACK,      │
│                          capability negotiation, QoS     │
├─────────────────────────────────────────────────────────┤
│  L1  Identity & Trust    Ed25519 DID, time-decayed      │
│                          trust scoring, Sybil guard     │
└─────────────────────────────────────────────────────────┘
```

### Layer 1: Identity & Trust

- **W3C DID** identities (`did:key:z6Mk...`) with Ed25519 cryptographic keys
- **Time-decayed trust scoring:**

```
T(a) = Σ(qᵢ × e^(-λΔt) × γ(task)) / Σ(e^(-λΔt) × γ(task))
```

  where λ = 0.01/day and γ maps task types to complexity weights
- **Sybil resistance** via transitive trust dampening (α = 0.5, max 5 hops)
- 31 tests

### Layer 2: Capability Handshake

- **3-phase SYN / SYN-ACK / ACK** negotiation inspired by TCP
- Agents declare capabilities (task type, quality, latency, cost)
- Binding **QoS contracts** with constraints: min quality, max latency, max cost
- 25 tests

### Layer 3: Semantic Context Differentials (SCD)

- **28x context compression** by extracting Minimal Sufficient Context (MSC)
- Hash-based embeddings → cosine similarity scoring → relevance-based chunking
- **Adaptive context**: iterative refinement when confidence < 0.7
- Configurable: relevance threshold, max chunks, chunk size, dimensions

```
MSC = {(chunk, score) : cosine(e_task, e_chunk) > threshold}
```

- 45 tests

### Layer 4: Economic Routing

- **Modified Bellman-Ford** with 10 Pareto weight vectors
- **Multi-objective optimization:** quality (multiplicative), latency (additive), cost (additive)
- **5 routing patterns:**

| Pattern | Strategy | Savings |
|---------|----------|---------|
| DraftRefine | Cheap agent drafts, specialist refines | 40-70% |
| Cascade | Try cheapest first, escalate on low confidence | 30-50% |
| ParallelMerge | Multiple agents process, merge results | Quality focus |
| Ensemble | Multiple agents vote on result | Reliability |
| Pipeline | Sequential processing chain | Throughput |

- Pareto-optimal route selection with constraint satisfaction
- 27 tests

### Layer 5: Fault Tolerance

- **Circuit breaker** with half-open recovery probes
- **Heartbeat monitoring** with < 100ms failure detection
- **Checkpoint/restore** for long-running tasks
- **Poison pill detection** for permanently failing inputs
- 42 tests

---

## Benchmarks

50 agents, 10,000 tasks, seed=42. All numbers reproducible.

```
Scenario             Cost/Task  Latency  Quality  Recovery    Ctx  Failed
─────────────────────────────────────────────────────────────────────────
Sequential            $0.0844    800ms    0.837       inf   1.0x       0
Round-Robin           $0.0712    720ms    0.856       inf   1.0x       0
ATP (full)            $0.0393    568ms    0.904       0ms  28.0x       0
ATP w/o SCD           $0.0627    612ms    0.891       0ms   1.0x       0
ATP w/o Routing       $0.0458    645ms    0.878       0ms  28.0x       0
ATP w/o Trust         $0.0451    634ms    0.892       0ms  28.0x       0
ATP w/o Fault         $0.0397    580ms    0.902       inf  28.0x       2
─────────────────────────────────────────────────────────────────────────

ATP vs Sequential:
  Cost:    -53.4%
  Latency: -29.0%
  Quality: +0.067
```

### Ablation Analysis

Every layer contributes. Removing any layer degrades results:

- **Without SCD (L3):** Cost jumps from $0.039 → $0.063 (+59%), compression drops to 1.0x
- **Without Routing (L4):** Cost rises to $0.046, quality drops to 0.878
- **Without Trust (L1):** Quality drops to 0.892, cost increases to $0.045
- **Without Fault (L5):** 2 task failures appear, recovery becomes infinite

---

## SDK API

The SDK provides two flavors of every operation:

| Flavor | Style | Returns | Use case |
|--------|-------|---------|----------|
| **Verb** | `route("coding")` | Prints to stdout | Quick exploration, demos |
| **Noun** | `find_route("coding")` | Typed result | Production code, pipelines |

Every type implements `Display` — just `println!("{result}")` and it formats beautifully.

### Functions

```rust
// ── Verb functions (print) ──────────────────────────────
atp_sdk::benchmark();                         // Full 7-scenario table
atp_sdk::route("coding");                     // Print best route
atp_sdk::compress(data, "coding");            // Print compression stats
atp_sdk::sign(b"hello");                      // Print agent + signature
atp_sdk::trust("coding");                     // Print trust score

// ── Noun functions (return values) ──────────────────────
let report = atp_sdk::bench(10_000);          // -> BenchReport
let route  = atp_sdk::find_route("coding");   // -> RouteResult
let route  = atp_sdk::find_route_with("coding", 0.9); // with min quality
let comp   = atp_sdk::shrink(data, "coding"); // -> CompressResult
let agent  = atp_sdk::agent();                // -> Agent (Ed25519 keypair)
let trust  = atp_sdk::trust_score("coding");  // -> TrustInfo
```

### Return Types

```rust
// RouteResult
route.task          // "CodeGeneration"
route.pattern       // "draft_refine"
route.agents        // 2
route.quality       // 0.92
route.cost          // 0.0500
route.latency_ms    // 45

// CompressResult
comp.ratio           // 28.3
comp.original_size   // 50000
comp.compressed_size // 1768
comp.chunks          // 3
comp.confidence      // 0.85

// Agent
agent.did()          // "did:key:z6Mk..."
agent.sign(msg)      // -> Signature
agent.verify(msg, &sig) // -> bool

// TrustInfo
trust.score          // 0.87
trust.samples        // 42
```

### Skill Aliases

Tasks are specified as simple strings. Case-insensitive with many aliases:

| Canonical | Aliases |
|-----------|---------|
| `"coding"` | `"code"`, `"codegen"`, `"code_generation"`, `"cg"` |
| `"analysis"` | `"analyze"`, `"analyse"` |
| `"writing"` | `"creative"`, `"creative_writing"`, `"cw"` |
| `"data"` | `"processing"`, `"data_processing"`, `"dp"` |

---

## Why ATP?

**MCP** discovers tools. **RAG** retrieves chunks. **ATP** orchestrates entire agent economies.

| Capability | MCP | RAG | ATP |
|------------|-----|-----|-----|
| Cryptographic Identity | - | - | Ed25519 DID |
| Trust Scoring | - | - | Time-decayed |
| Sybil Resistance | - | - | Transitive dampening |
| Capability Negotiation | Basic | - | 3-phase handshake |
| Context Compression | - | Chunk retrieval | 28x SCD |
| Multi-Agent Routing | - | - | 5 patterns |
| Economic Optimization | - | - | Pareto-optimal |
| Fault Tolerance | - | - | Circuit breaker |
| Heartbeat Monitoring | - | - | < 100ms detection |
| QoS Contracts | - | - | Binding |

ATP doesn't replace MCP or RAG — it provides the **networking layer** they lack. Think of it as the difference between an app (MCP/RAG) and the network protocol (ATP) that apps run on.

---

## Project Structure

```
AgentTransportProtocol/
├── Cargo.toml              # Workspace root
├── render.yaml             # Render deployment config
├── proto/atp/v1/           # 8 protobuf definitions
│   ├── common.proto        #   Shared types (TaskType, QoS, Capability)
│   ├── identity.proto      #   DID, trust, interaction proofs
│   ├── handshake.proto     #   SYN/SYN-ACK/ACK messages
│   ├── context.proto       #   Context diffs, embeddings
│   ├── routing.proto       #   Route queries, responses
│   ├── fault.proto         #   Heartbeat, circuit break
│   ├── task.proto          #   Task submission, results
│   └── service.proto       #   gRPC service definition
├── crates/
│   ├── atp-types/          # Core types, traits, error hierarchy
│   ├── atp-proto/          # Generated protobuf + tonic code
│   ├── atp-identity/       # L1: DID, Ed25519, trust, Sybil guard
│   ├── atp-handshake/      # L2: 3-phase capability handshake
│   ├── atp-context/        # L3: SCD compression, cosine similarity
│   ├── atp-routing/        # L4: Bellman-Ford, 5 routing patterns
│   ├── atp-fault/          # L5: Circuit breaker, heartbeat, checkpoint
│   ├── atp-transport/      # gRPC server/client stubs
│   ├── atp-node/           # Composition root (wires all layers)
│   ├── atp-sim/            # Simulation framework (agents, network)
│   ├── atp-bench/          # AgentNet-Bench CLI
│   ├── atp-sdk/            # Public facade — dead-simple API
│   └── atp-python/         # Python bindings (PyO3) [excluded]
└── website/
    └── index.html          # Marketing site with interactive playground
```

---

## Building from Source

### Prerequisites

- Rust 1.75+ (`rustup install stable`)
- Protobuf compiler (`brew install protobuf` or `apt install protobuf-compiler`)

### Build & Test

```bash
git clone https://github.com/rajamohan1950/AgentTransportProtocol.git
cd AgentTransportProtocol

export PROTOC=$(which protoc)

cargo build --workspace          # Build all 12 crates
cargo test --workspace           # Run all 280 tests
cargo clippy --workspace         # Zero warnings

# Run benchmark
cargo run --release -p atp-bench -- --agents 50 --tasks 10000 --seed 42

# Output formats
cargo run --release -p atp-bench -- --output json   # JSON output
cargo run --release -p atp-bench -- --output csv    # CSV output

# Single scenario
cargo run --release -p atp-bench -- --scenario atp
```

### Python SDK

```bash
pip install maturin
cd crates/atp-python
maturin develop --release
python -c "import atp; atp.benchmark()"
```

---

## gRPC Service

ATP defines a full gRPC service for networked agent communication:

```protobuf
service AtpService {
  rpc Probe(CapabilityProbe) returns (CapabilityOffer);         // L2 handshake
  rpc AcceptContract(ContractAccept) returns (ContractAck);     // L2 QoS
  rpc SubmitTask(TaskSubmit) returns (TaskAck);                 // Task lifecycle
  rpc StreamResults(TaskQuery) returns (stream TaskResult);     // Streaming results
  rpc RequestContext(ContextRequest) returns (ContextResponse); // L3 context
  rpc QueryRoute(RouteQuery) returns (RouteResponse);           // L4 routing
  rpc SendHeartbeat(Heartbeat) returns (HeartbeatAck);          // L5 heartbeat
  rpc ReportCircuitBreak(CircuitBreak) returns (CircuitBreakAck); // L5 circuit break
  rpc SubmitInteractionProof(InteractionProof) returns (ProofAck); // L1 trust
}
```

---

## Contributing

Contributions welcome! Areas of interest:

- **Wire protocol**: Flesh out `atp-transport` with full gRPC server/client
- **Python SDK**: Expand PyO3 bindings for all layers
- **Integration tests**: Cross-layer end-to-end scenarios
- **Benchmarks**: Real-world agent workloads and comparisons
- **Documentation**: More examples, tutorials, and guides

---

## License

Dual licensed under [MIT](LICENSE-MIT) and [Apache 2.0](LICENSE-APACHE) — choose whichever you prefer.

---

## Author

**Rajamohan Jabbala** — [AlphaForge AI Labs](https://github.com/rajamohan1950)

---

<p align="center">
  <strong>280 tests &bull; ~37,000 lines of Rust &bull; Zero dependencies for users &bull; Built with Rust 🦀</strong>
</p>
