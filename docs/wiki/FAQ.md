# Frequently Asked Questions

---

## General

### What is ATP?
ATP (Agent Transport Protocol) is a five-layer protocol stack for multi-agent communication. Think of it as the TCP/IP of AI agents — it provides identity, trust, handshaking, context compression, economic routing, and fault tolerance for agent networks.

### Is ATP an LLM framework?
No. ATP is a **networking protocol** for agents. It doesn't include an LLM — it provides the infrastructure that allows agents (powered by any LLM) to communicate, trust each other, and collaborate efficiently.

### Does ATP replace MCP or RAG?
No. ATP is complementary. MCP handles tool discovery, RAG handles knowledge retrieval, and ATP handles multi-agent networking. You can use all three together. See [[ATP vs MCP vs RAG]] for a detailed comparison.

### What language is ATP written in?
Rust. The entire codebase is ~37,000 lines of Rust plus 308 lines of Protobuf definitions. A Python SDK via PyO3 is also available.

### Is ATP production-ready?
The core protocol, SDK, and benchmarks are solid with 280 tests and zero failures. The gRPC transport layer (`atp-transport`) is still in stub form and needs to be fleshed out for production networked deployment.

---

## Technical

### How does the 28x compression work?
ATP's Layer 3 (Semantic Context Differentials) extracts only the semantically relevant portions of context for each task. It chunks the input, generates embeddings, computes cosine similarity against the task embedding, and keeps only chunks above a relevance threshold. For a typical 50KB context, only ~1.8KB is task-relevant. See [[Layer 3: Context Compression]].

### How fast are routing decisions?
Sub-microsecond. The modified Bellman-Ford algorithm runs in O(k² × |W|) where k is the number of capability-matched agents (~12) and |W| = 10 weight vectors. That's ~1,440 iterations — trivial for modern CPUs.

### What cryptography does ATP use?
Ed25519 via the `ed25519-dalek` crate. Agent identities are W3C DID URIs (`did:key:z6Mk...`). All interactions can be cryptographically signed and verified.

### Are benchmarks reproducible?
Yes. All benchmarks use seeded random number generators. Running with `--seed 42` produces identical results every time.

### How does trust scoring work?
Trust is a weighted moving average with exponential time decay. Recent interactions count more, complex tasks (coding γ=1.5) influence trust more than simple tasks (data γ=0.8), and Sybil attacks are mitigated by transitive dampening (α=0.5). See [[Layer 1: Identity and Trust]].

### What are the 5 routing patterns?
1. **DraftRefine** — cheap agent drafts, specialist refines (40-70% savings)
2. **Cascade** — try cheapest first, escalate on low confidence (30-50% savings)
3. **ParallelMerge** — multiple agents process, merge results
4. **Ensemble** — multiple agents vote on result
5. **Pipeline** — sequential processing chain

See [[Layer 4: Economic Routing]].

---

## Usage

### How do I install ATP?
Add to your `Cargo.toml`:
```toml
[dependencies]
atp-sdk = { git = "https://github.com/rajamohan1950/AgentTransportProtocol" }
```

### What's the simplest possible usage?
```rust
fn main() {
    atp_sdk::benchmark();
}
```
One line. Zero config. That's it.

### Can I use ATP from Python?
Yes. Install with `maturin`:
```bash
cd crates/atp-python && maturin develop --release
python -c "import atp; atp.benchmark()"
```

### How do I run the benchmark CLI?
```bash
cargo run --release -p atp-bench -- --agents 50 --tasks 10000 --seed 42
```

### What task types does ATP support?
Four types: `"coding"`, `"analysis"`, `"writing"`, `"data"`. Each accepts many aliases (e.g., `"code"`, `"cg"`, `"codegen"` all map to coding). Case-insensitive.

---

## Architecture

### Why five layers?
Each layer solves a distinct problem: identity (who are you?), handshake (what can you do?), compression (what context do you need?), routing (who should do what?), fault tolerance (what if something breaks?). The ablation benchmarks prove each layer adds measurable value — removing any one degrades results.

### Why not use existing networking protocols?
TCP/IP and HTTP handle byte streams between machines. Agent communication requires higher-level abstractions: trust, capabilities, semantic context, economic optimization. ATP operates at the agent level, not the byte level.

### Can I use individual layers without the full stack?
Yes. Each layer is a separate Rust crate. You can use `atp-identity` for trust scoring without any other layer, or `atp-context` for compression standalone.

### What's the wire protocol?
gRPC with Protobuf. 8 proto files define the full wire format. See [[gRPC Service]].

---

## Contributing

### How can I contribute?
See [[Contributing]] for detailed instructions. High-priority areas: gRPC transport, integration tests, Python SDK expansion.

### What's the license?
Dual MIT / Apache 2.0 — choose whichever you prefer.

### Who created ATP?
**Rajamohan Jabbala** at AlphaForge AI Labs.
