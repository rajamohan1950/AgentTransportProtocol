# Contributing

We welcome contributions to ATP! Here's how to get involved.

---

## Getting Set Up

```bash
git clone https://github.com/rajamohan1950/AgentTransportProtocol.git
cd AgentTransportProtocol

export PROTOC=$(which protoc)

cargo build --workspace
cargo test --workspace     # All 280 tests must pass
cargo clippy --workspace   # Zero warnings required
```

## Areas of Interest

### High Priority

| Area | Crate | Description |
|------|-------|-------------|
| **gRPC Server** | `atp-transport` | Flesh out the full gRPC server with all 10 RPCs |
| **gRPC Client** | `atp-transport` | Client-side implementation for networked agents |
| **Integration Tests** | (new) | Cross-layer end-to-end test scenarios |
| **Python SDK** | `atp-python` | Expand PyO3 bindings for all layer functions |

### Medium Priority

| Area | Crate | Description |
|------|-------|-------------|
| Real-world benchmarks | `atp-bench` | Benchmarks with actual LLM API calls |
| Persistent storage | `atp-identity` | Database-backed identity and trust store |
| WebSocket transport | `atp-transport` | Alternative to gRPC for browser environments |
| Observability | (new) | Metrics, tracing, and logging across layers |

### Research

| Area | Description |
|------|-------------|
| Embedding models | Replace hash-based embeddings with real models in L3 |
| Adaptive routing | Reinforcement learning for route pattern selection |
| Federated trust | Cross-network trust propagation |
| Formal verification | Prove correctness of handshake and routing protocols |

## Code Style

- **Rust edition:** 2021
- **Formatting:** `cargo fmt`
- **Linting:** `cargo clippy` with zero warnings
- **Tests:** Every public function must have tests
- **Documentation:** Doc comments on all public items

## Pull Request Process

1. Fork the repository
2. Create a feature branch: `git checkout -b feat/my-feature`
3. Write your code and tests
4. Ensure all checks pass:
   ```bash
   cargo fmt --check
   cargo clippy --workspace
   cargo test --workspace
   ```
5. Open a PR with a clear description of what and why

## Project Structure

```
crates/
├── atp-types/      # Core types — change carefully, everything depends on this
├── atp-proto/      # Generated code — modify proto/ files, not the generated code
├── atp-identity/   # Layer 1 — trust formulas here
├── atp-handshake/  # Layer 2 — handshake state machine here
├── atp-context/    # Layer 3 — compression algorithms here
├── atp-routing/    # Layer 4 — Bellman-Ford and patterns here
├── atp-fault/      # Layer 5 — circuit breaker logic here
├── atp-transport/  # gRPC stubs — NEEDS WORK
├── atp-node/       # Composition root — NEEDS WORK
├── atp-sim/        # Simulation — add new agent behaviors here
├── atp-bench/      # Benchmarks — add new scenarios here
├── atp-sdk/        # Public facade — keep it simple
└── atp-python/     # Python bindings — built separately with maturin
```

## Questions?

Open an issue on GitHub or reach out to [Rajamohan Jabbala](https://github.com/rajamohan1950).
