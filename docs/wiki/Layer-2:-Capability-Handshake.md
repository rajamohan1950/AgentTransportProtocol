# Layer 2: Capability Handshake

**Crate:** `atp-handshake` | **Tests:** 25

Layer 2 provides 3-phase capability negotiation with binding QoS contracts, inspired by TCP's handshake but designed for agent economies.

---

## The 3-Phase Handshake

```
Agent A                    Agent B
   │                          │
   │─── SYN (CapabilityProbe) ──→│   "What can you do?"
   │                          │
   │←── SYN-ACK (CapabilityOffer) ──│   "Here's what I offer"
   │                          │
   │─── ACK (ContractAccept) ──→│   "Deal! Here's the QoS contract"
   │                          │
   │←── ContractAck ──────────│   "Contract confirmed"
   │                          │
```

### Phase 1: Probe (SYN)

The requesting agent sends a `CapabilityProbe`:
- What task type do you handle?
- What are your QoS requirements? (min quality, max latency, max cost)

### Phase 2: Offer (SYN-ACK)

The receiving agent responds with a `CapabilityOffer`:
- Task types I support
- My estimated quality per task type
- My estimated latency per task type
- My cost per task

### Phase 3: Accept (ACK)

If the offer meets QoS constraints, a binding `ContractAccept` is sent:
- Selected task type
- Agreed quality threshold
- Agreed latency bound
- Agreed cost

## QoS Contracts

Every handshake results in a **binding QoS contract**:

```rust
pub struct QoSConstraints {
    pub min_quality: f64,      // e.g., 0.8
    pub max_latency_ms: u64,   // e.g., 1000
    pub max_cost: f64,         // e.g., 0.10
    pub min_trust: f64,        // e.g., 0.5
}
```

Agents that consistently violate their QoS contracts will see their trust scores decrease via Layer 1.

## Capability Declaration

Agents declare their capabilities as structured data:

```rust
pub struct Capability {
    pub task_type: TaskType,
    pub estimated_quality: f64,
    pub estimated_latency_ms: u64,
    pub cost_per_task: f64,
}
```

A single agent can declare multiple capabilities (e.g., coding at quality 0.9 and analysis at quality 0.7).

## Why This Matters

Without handshakes, agents are assigned tasks blindly. Layer 2 ensures:
- **No surprises**: Both sides agree on expectations before work begins
- **Quality guarantees**: Minimum quality thresholds are contractually binding
- **Cost control**: Maximum costs are agreed upfront
- **Trust integration**: Only agents above the trust threshold are considered

## Next Steps

- [[Layer 3: Context Compression]] — How context is compressed before transfer
- [[Architecture Overview]] — See how handshake fits in the stack
