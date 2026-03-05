# Agent Transport Protocol (ATP) — Wiki

**The TCP/IP of AI Agents**

Welcome to the ATP Wiki — the comprehensive guide to understanding, using, and contributing to the Agent Transport Protocol.

---

## What is ATP?

ATP is a **five-layer protocol stack** for trust-aware, economically-optimal multi-agent networking. It provides the missing networking layer that allows AI agents to:

- **Identify** each other with cryptographic DIDs
- **Trust** each other with time-decayed scoring
- **Negotiate** capabilities with binding QoS contracts
- **Compress** context by 28x using semantic differentials
- **Route** tasks optimally across agent economies
- **Recover** from failures with circuit breakers and heartbeats

## Quick Navigation

| Page | Description |
|------|-------------|
| [[Getting Started]] | Install, build, and run your first ATP program |
| [[Architecture Overview]] | The five-layer protocol stack explained |
| [[Layer 1: Identity and Trust]] | Ed25519 DID, trust scoring, Sybil resistance |
| [[Layer 2: Capability Handshake]] | 3-phase negotiation and QoS contracts |
| [[Layer 3: Context Compression]] | Semantic Context Differentials (SCD) |
| [[Layer 4: Economic Routing]] | Bellman-Ford, 5 routing patterns |
| [[Layer 5: Fault Tolerance]] | Circuit breaker, heartbeat, checkpoint |
| [[SDK API Reference]] | Complete function reference for atp-sdk |
| [[Python SDK]] | PyO3 bindings and Python usage |
| [[Benchmarks]] | AgentNet-Bench results and analysis |
| [[gRPC Service]] | Protobuf definitions and wire protocol |
| [[ATP vs MCP vs RAG]] | Why ATP exists and how it compares |
| [[Case Studies]] | Real-world use cases |
| [[Contributing]] | How to contribute to ATP |
| [[FAQ]] | Frequently asked questions |

## Key Numbers

```
Cost Reduction:     -53.4% vs sequential
Context Compression: 28x via SCD
Task Failures:       0 / 10,000
Quality Score:       0.904 (+8% over baselines)
Latency Reduction:  -29.3%
Routing Decisions:  < 1 microsecond
Tests:               280 passing, zero failures
Lines of Rust:      ~37,000 across 75 files
```

## Links

- [GitHub Repository](https://github.com/rajamohan1950/AgentTransportProtocol)
- [Website with Interactive Playground](https://atp-website.onrender.com)

---

*Created by **Rajamohan Jabbala** — AlphaForge AI Labs*
