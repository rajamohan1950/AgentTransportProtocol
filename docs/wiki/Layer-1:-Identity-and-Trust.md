# Layer 1: Identity and Trust

**Crate:** `atp-identity` | **Tests:** 31

Layer 1 provides cryptographic identity, time-decayed trust scoring, and Sybil resistance for agent networks.

---

## Components

### 1. Cryptographic Identity (DID)

Every agent gets a **W3C DID** (Decentralized Identifier) backed by an Ed25519 keypair:

```
did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK
```

**Implementation:**
- `DidGenerator` — generates `did:key` identifiers from Ed25519 public keys
- `KeyPair` — Ed25519 key management (sign, verify, serialize)
- Keys are generated using the `ed25519-dalek` crate with secure randomness

**Usage via SDK:**
```rust
let agent = atp_sdk::agent();
println!("{}", agent.did());     // "did:key:z6Mk..."
let sig = agent.sign(b"hello");
assert!(agent.verify(b"hello", &sig));
```

### 2. Trust Scoring

Trust is computed as a **weighted moving average** with exponential time decay:

```
T(a) = Σ(qᵢ × w(τᵢ) × γ(taskᵢ)) / Σ(w(τᵢ) × γ(taskᵢ))

where:
  qᵢ        = quality score of interaction i (0.0 to 1.0)
  w(τᵢ)     = e^(-λ × Δdays)   [time-decay weight]
  λ          = 0.01 per day     [decay rate]
  γ(task)    = complexity weight for task type
  Δdays      = days since interaction
```

**Properties:**
- Recent interactions matter more (exponential decay)
- Complex tasks (coding γ=1.5) influence trust more than simple tasks (data γ=0.8)
- Bayesian prior of 0.5 for agents with no history
- Score range: 0.0 (untrusted) to 1.0 (fully trusted)

**Complexity Weights:**

| Task Type | γ weight |
|-----------|----------|
| CodeGeneration | 1.5 |
| Analysis | 1.2 |
| CreativeWriting | 1.0 |
| DataProcessing | 0.8 |

### 3. Sybil Resistance

**Transitive Trust Dampening** prevents agents from creating fake identities to boost their scores:

```
T_transitive(a→c) = T(a→b) × T(b→c) × α^depth

where:
  α     = 0.5   [dampening factor]
  depth = number of hops in the trust chain
  max   = 5 hops
```

At each hop, trust is dampened by 50%. After 5 hops, transitive trust ≈ 3% of direct trust, making Sybil attacks economically infeasible.

### 4. Identity Store

The `IdentityStore` provides persistent storage for:
- Agent registrations (DID + public key)
- Interaction records (quality, timestamp, task type)
- Trust score caching and invalidation

## Key Types

```rust
pub struct TrustEngine {
    pub fn new(lambda: f64, prior: f64) -> Self
    pub fn compute_trust_score(
        &self,
        subject: AgentId,
        task_type: TaskType,
        records: &[InteractionRecord],
        now: DateTime<Utc>,
    ) -> TrustScore
}

pub struct TrustScore {
    pub score: f64,        // 0.0 to 1.0
    pub confidence: f64,   // Based on sample count
    pub sample_count: u32, // Number of interactions
}

pub struct InteractionRecord {
    pub agent_id: AgentId,
    pub quality: f64,
    pub task_type: TaskType,
    pub timestamp: DateTime<Utc>,
}
```

## Why This Matters

Without trust scoring, any agent can claim to be excellent at any task. ATP's trust layer ensures:
- **Accountability**: Bad quality degrades trust over time
- **Freshness**: Trust reflects recent performance, not historical
- **Sybil safety**: Creating fake identities doesn't help
- **Task specificity**: An agent trusted for coding isn't automatically trusted for analysis

## Next Steps

- [[Layer 2: Capability Handshake]] — How trusted agents negotiate
- [[SDK API Reference]] — `agent()`, `sign()`, `trust()` functions
