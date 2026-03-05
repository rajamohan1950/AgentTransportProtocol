# Getting Started

Get ATP running in under 30 seconds.

---

## Prerequisites

| Tool | Version | Install |
|------|---------|---------|
| Rust | 1.75+ | `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \| sh` |
| Protobuf | 3.x+ | `brew install protobuf` or `apt install protobuf-compiler` |

## Installation

### Option 1: As a Dependency (Recommended)

Add to your `Cargo.toml`:

```toml
[dependencies]
atp-sdk = { git = "https://github.com/rajamohan1950/AgentTransportProtocol" }
```

### Option 2: Clone and Build

```bash
git clone https://github.com/rajamohan1950/AgentTransportProtocol.git
cd AgentTransportProtocol

export PROTOC=$(which protoc)

cargo build --workspace
cargo test --workspace     # 280 tests, all passing
```

## Your First ATP Program

Create a new Rust project:

```bash
cargo new my-agent-app
cd my-agent-app
```

Add the dependency:

```toml
# Cargo.toml
[dependencies]
atp-sdk = { git = "https://github.com/rajamohan1950/AgentTransportProtocol" }
```

Write your program:

```rust
// src/main.rs
fn main() {
    // Run the full benchmark — 50 agents, 10K tasks, 7 scenarios
    atp_sdk::benchmark();

    // Find the best route for a coding task
    atp_sdk::route("coding");

    // Compress context data
    atp_sdk::compress(b"your context data here...", "analysis");

    // Create a cryptographic identity and sign a message
    atp_sdk::sign(b"hello world");

    // Check network trust for a skill
    atp_sdk::trust("coding");
}
```

Run it:

```bash
cargo run
```

That's it. **No config files. No setup. No structs to create.** Just call the function.

## Using Return Values

For production code, use the "noun" functions that return typed results:

```rust
fn main() {
    // Get structured benchmark data
    let report = atp_sdk::bench(10_000);
    println!("ATP cost: ${:.4}", report.atp().unwrap().avg_cost_per_task);

    // Get route details
    let route = atp_sdk::find_route("coding");
    if route.quality > 0.9 {
        println!("High quality route found: {}", route.pattern);
    }

    // Get compression metrics
    let comp = atp_sdk::shrink(b"data...", "analysis");
    println!("Compressed {}x", comp.ratio);

    // Create an identity
    let agent = atp_sdk::agent();
    let sig = agent.sign(b"message");
    assert!(agent.verify(b"message", &sig));
    println!("Agent DID: {}", agent.did());
}
```

## Running the Benchmark CLI

```bash
# Default: 50 agents, 10K tasks
cargo run --release -p atp-bench

# Custom parameters
cargo run --release -p atp-bench -- --agents 100 --tasks 50000 --seed 123

# JSON output (for pipelines)
cargo run --release -p atp-bench -- --output json

# CSV output (for spreadsheets)
cargo run --release -p atp-bench -- --output csv

# Single scenario
cargo run --release -p atp-bench -- --scenario atp
```

## Next Steps

- [[Architecture Overview]] — Understand the five layers
- [[SDK API Reference]] — Complete function reference
- [[Benchmarks]] — Detailed performance analysis
- [[Python SDK]] — Use ATP from Python
