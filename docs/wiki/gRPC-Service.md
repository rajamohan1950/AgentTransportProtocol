# gRPC Service

**Crate:** `atp-proto` (generated) + `atp-transport` (stubs) | **Protobuf:** 8 files, 308 lines

ATP defines a complete gRPC service for networked agent communication, with RPCs spanning all 5 protocol layers.

---

## Service Definition

```protobuf
service AtpService {
  // Layer 2: Capability Handshake
  rpc Probe(CapabilityProbe) returns (CapabilityOffer);
  rpc AcceptContract(ContractAccept) returns (ContractAck);

  // Task Lifecycle
  rpc SubmitTask(TaskSubmit) returns (TaskAck);
  rpc StreamResults(TaskQuery) returns (stream TaskResult);

  // Layer 3: Context
  rpc RequestContext(ContextRequest) returns (ContextResponse);

  // Layer 4: Routing
  rpc QueryRoute(RouteQuery) returns (RouteResponse);

  // Layer 5: Fault Tolerance
  rpc SendHeartbeat(Heartbeat) returns (HeartbeatAck);
  rpc ReportBackpressure(Backpressure) returns (BackpressureAck);
  rpc ReportCircuitBreak(CircuitBreak) returns (CircuitBreakAck);

  // Layer 1: Trust
  rpc SubmitInteractionProof(InteractionProof) returns (ProofAck);
}
```

## Common Types

### TaskType
```protobuf
enum TaskType {
  TASK_TYPE_UNSPECIFIED = 0;
  TASK_TYPE_CODE_GENERATION = 1;
  TASK_TYPE_ANALYSIS = 2;
  TASK_TYPE_CREATIVE_WRITING = 3;
  TASK_TYPE_DATA_PROCESSING = 4;
}
```

### QoSConstraints
```protobuf
message QoSConstraints {
  double min_quality = 1;       // Minimum acceptable quality (0-1)
  uint64 max_latency_ms = 2;   // Maximum acceptable latency
  double max_cost = 3;          // Maximum acceptable cost (USD)
  double min_trust = 4;         // Minimum trust score required
}
```

### Capability
```protobuf
message Capability {
  TaskType task_type = 1;
  double estimated_quality = 2;
  uint64 estimated_latency_ms = 3;
  double cost_per_task = 4;
}
```

## Proto Files

| File | Layer | Description |
|------|-------|-------------|
| `common.proto` | Shared | TaskType, QoSConstraints, Capability |
| `identity.proto` | L1 | DID, trust proofs, interaction records |
| `handshake.proto` | L2 | Probe, Offer, Accept, Contract messages |
| `context.proto` | L3 | Context diffs, embeddings, compression |
| `routing.proto` | L4 | Route queries, responses, patterns |
| `fault.proto` | L5 | Heartbeat, circuit break, backpressure |
| `task.proto` | - | Task submission, results, lifecycle |
| `service.proto` | All | The AtpService definition |

## Code Generation

Protobuf code is generated at build time via `atp-proto/build.rs`:

```rust
// build.rs
fn main() {
    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .compile(
            &["proto/atp/v1/service.proto"],
            &["proto"],
        )
        .unwrap();
}
```

## Using Generated Code

```rust
use atp_proto::atp::v1::*;

// Create a capability probe
let probe = CapabilityProbe {
    task_type: TaskType::CodeGeneration as i32,
    qos: Some(QoSConstraints {
        min_quality: 0.8,
        max_latency_ms: 1000,
        max_cost: 0.10,
        min_trust: 0.5,
    }),
};
```

## Current Status

The gRPC service definitions are complete. The `atp-transport` crate contains server and client stubs that are ready to be fleshed out for production use. See [[Contributing]] for how to help.

## Next Steps

- [[Architecture Overview]] — How gRPC fits in the stack
- [[Contributing]] — Help build the transport layer
