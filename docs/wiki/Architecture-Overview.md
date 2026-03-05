# Architecture Overview

ATP is a five-layer protocol stack inspired by the OSI model but purpose-built for AI agent communication.

---

## The Five Layers

```
┌──────────────────────────────────────────────────────────────┐
│  L5  FAULT TOLERANCE                                         │
│      Circuit breaker, heartbeat, checkpoint, poison pill     │
│      Crate: atp-fault  │  42 tests                          │
├──────────────────────────────────────────────────────────────┤
│  L4  ECONOMIC ROUTING                                        │
│      Bellman-Ford, 5 patterns, Pareto optimization           │
│      Crate: atp-routing  │  27 tests                        │
├──────────────────────────────────────────────────────────────┤
│  L3  SEMANTIC CONTEXT DIFFERENTIALS (SCD)                    │
│      28x compression, cosine similarity, MSC extraction      │
│      Crate: atp-context  │  45 tests                        │
├──────────────────────────────────────────────────────────────┤
│  L2  CAPABILITY HANDSHAKE                                    │
│      3-phase SYN/SYN-ACK/ACK, QoS contracts                 │
│      Crate: atp-handshake  │  25 tests                      │
├──────────────────────────────────────────────────────────────┤
│  L1  IDENTITY & TRUST                                        │
│      Ed25519 DID, time-decayed trust, Sybil resistance       │
│      Crate: atp-identity  │  31 tests                       │
└──────────────────────────────────────────────────────────────┘
```

## Design Principles

### 1. Layer Independence
Each layer operates independently. You can use economic routing (L4) without fault tolerance (L5), or trust scoring (L1) without context compression (L3). The ablation benchmarks prove each layer adds measurable value.

### 2. Zero-Config Defaults
Every layer has sensible defaults. The SDK facade (`atp-sdk`) eliminates all configuration by using a global lazy-initialized network with 50 simulated agents.

### 3. Pareto-Optimal Decisions
Routing doesn't optimize a single metric — it explores the Pareto frontier across quality, latency, and cost simultaneously using 10 weight vectors.

### 4. Cryptographic Foundation
All agent identities are Ed25519 keypairs with W3C DID URIs. Every interaction can be cryptographically signed and verified.

### 5. Fault-First Design
The protocol assumes agents will fail. Circuit breakers, heartbeats, poison pill detection, and checkpointing are built in, not bolted on.

## Crate Dependency Graph

```
atp-sdk (public facade)
  ├── atp-sim (simulation framework)
  │     ├── atp-routing (L4)
  │     │     ├── atp-types
  │     │     └── atp-identity (L1, for trust scores)
  │     ├── atp-context (L3)
  │     ├── atp-fault (L5)
  │     ├── atp-handshake (L2)
  │     └── atp-types (core types)
  ├── atp-identity (L1, for Agent/DID)
  ├── atp-context (L3, for compression)
  ├── atp-routing (L4, for routing)
  └── atp-types (core types)
```

## Supporting Crates

| Crate | Purpose |
|-------|---------|
| `atp-types` | Core types, traits, and error hierarchy. Zero logic — only definitions. |
| `atp-proto` | Generated protobuf + tonic code from `proto/atp/v1/*.proto`. Built via `build.rs`. |
| `atp-transport` | gRPC server and client stubs for networked agent communication. |
| `atp-node` | Composition root that wires all layers together into a running node. |
| `atp-sim` | Simulation framework with simulated agents, network topology, and clock. |
| `atp-bench` | AgentNet-Bench CLI that runs 7 scenarios and produces comparison tables. |
| `atp-sdk` | Public facade with dead-simple free functions. The entry point for users. |
| `atp-python` | Python bindings via PyO3 (excluded from workspace, built with maturin). |

## Task Types

ATP supports four task types, each with different complexity weights:

| Task Type | Weight (γ) | Description |
|-----------|------------|-------------|
| CodeGeneration | 1.5 | Code writing, refactoring, debugging |
| Analysis | 1.2 | Data analysis, research, evaluation |
| CreativeWriting | 1.0 | Content creation, creative tasks |
| DataProcessing | 0.8 | ETL, transformation, formatting |

Higher weights mean the task type has more influence on trust scoring.

## Wire Protocol

ATP defines a gRPC service (`AtpService`) with 10 RPCs spanning all 5 layers. See [[gRPC Service]] for the full protobuf definitions.

## Next Steps

- [[Layer 1: Identity and Trust]] — Deep dive into the trust layer
- [[Layer 2: Capability Handshake]] — How agents negotiate
- [[Layer 3: Context Compression]] — The 28x compression pipeline
- [[Layer 4: Economic Routing]] — Multi-objective optimization
- [[Layer 5: Fault Tolerance]] — Circuit breakers and recovery
