# Python SDK

**Crate:** `atp-python` | **Runtime:** PyO3 + maturin

The Python SDK provides native Python bindings to ATP via PyO3. Same power, same simplicity, Pythonic interface.

---

## Installation

```bash
# Prerequisites
pip install maturin

# Build and install
cd crates/atp-python
maturin develop --release

# Verify
python -c "import atp; atp.benchmark()"
```

## Quick Start

```python
import atp

# Run the full benchmark
atp.benchmark()

# Find the best route for coding tasks
atp.route("coding")

# Compress context data
atp.compress(b"your context data here...", "coding")

# Create an identity and sign a message
atp.sign(b"hello world")

# Check network trust
atp.trust("coding")
```

## Functions

### `atp.benchmark()`
Prints the full 7-scenario benchmark table. 50 agents, 10,000 tasks, all scenarios including ablation.

### `atp.route(skill: str)`
Finds and prints the optimal route for the given task type.

```python
atp.route("coding")     # → Route: draft_refine via 2 agents (q=0.92, $0.0500, 45ms)
atp.route("analysis")   # → Route: cascade via 3 agents (q=0.88, $0.0380, 52ms)
```

### `atp.compress(data: bytes, skill: str)`
Compresses context data using Semantic Context Differentials.

```python
data = b"A" * 50000  # 50KB of context
atp.compress(data, "coding")   # → 28x compression
```

### `atp.sign(message: bytes)`
Creates a new Ed25519 identity, signs the message, and verifies the signature.

```python
atp.sign(b"hello world")
# Agent:     did:key:z6Mk...
# Signature: a3f2b1c9...
# Verified:  true
```

### `atp.trust(skill: str)`
Shows the average trust score for the given task type across the network.

```python
atp.trust("coding")   # → Trust: 0.87 (n=42)
```

## Low-Level API

The Python SDK also exposes lower-level classes for advanced usage:

### DidGenerator
```python
from atp import DidGenerator

gen = DidGenerator()
did = gen.generate()
identity = gen.create_identity()
```

### IdentityStore
```python
from atp import IdentityStore

store = IdentityStore()
store.register(identity)
info = store.lookup(did_uri)
```

### ContextCompressor
```python
from atp import ContextCompressor

compressor = ContextCompressor()
result = compressor.compress(data, "coding")
```

### EconomicRouter
```python
from atp import EconomicRouter

router = EconomicRouter()
router.add_agent(agent_id, capabilities, trust_score=0.9)
route = router.find_route("coding", min_quality=0.8)
```

### Simulation
```python
from atp import Simulation

sim = Simulation(agents=50, seed=42)
results = sim.run_benchmark(tasks=10000)
for r in results:
    print(r)
```

## Skill Aliases

Same as the Rust SDK — case-insensitive with many aliases:

| Input | Maps to |
|-------|---------|
| `"coding"`, `"code"`, `"cg"` | CodeGeneration |
| `"analysis"`, `"analyze"` | Analysis |
| `"writing"`, `"creative"`, `"cw"` | CreativeWriting |
| `"data"`, `"processing"`, `"dp"` | DataProcessing |

## Next Steps

- [[SDK API Reference]] — Full Rust API reference
- [[Getting Started]] — Building from source
