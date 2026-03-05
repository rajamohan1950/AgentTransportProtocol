# Layer 3: Semantic Context Differentials (SCD)

**Crate:** `atp-context` | **Tests:** 45

Layer 3 provides **28x context compression** by extracting only the semantically relevant portions of context for each task.

---

## The Problem

When agents collaborate, they pass context between each other. A coding agent might send 50,000 tokens of context to a review agent. But the review agent only needs the relevant parts — perhaps 1,768 tokens. Sending everything wastes bandwidth, costs money, and can overwhelm receiving agents.

## The Solution: Minimal Sufficient Context (MSC)

SCD extracts the **Minimal Sufficient Context** — the smallest subset of context that preserves task-relevant information.

### Compression Pipeline

```
Input Context (50,000B)
       │
       ▼
┌──────────────┐
│ 1. Chunk     │  Split into fixed-size chunks (default 512B)
└──────┬───────┘
       ▼
┌──────────────┐
│ 2. Embed     │  Generate hash-based embeddings (64-dim)
└──────┬───────┘
       ▼
┌──────────────┐
│ 3. Score     │  Cosine similarity against task embedding
└──────┬───────┘
       ▼
┌──────────────┐
│ 4. Extract   │  Keep chunks where score > threshold (0.3)
└──────┬───────┘
       ▼
┌──────────────┐
│ 5. Package   │  Generate wire-format ContextDiff
└──────────────┘
       │
       ▼
Output Context (1,768B)  ← 28x smaller
```

### Core Formula

```
cos(a, b) = (a · b) / (|a| × |b|)

MSC = {(chunk, score) : cosine(e_task, e_chunk) > threshold}
```

Where:
- `e_task` = embedding of the task type
- `e_chunk` = embedding of each context chunk
- `threshold` = relevance cutoff (default 0.3)

## Configuration

```rust
pub struct MscConfig {
    pub relevance_threshold: f64,  // Default 0.3 — chunks below this are dropped
    pub max_chunks: usize,         // Default 10 — maximum chunks to retain
    pub chunk_size: usize,         // Default 512 — bytes per chunk
    pub dimensions: usize,         // Default 64 — embedding dimensions
}
```

## Adaptive Context

When compression confidence is below 0.7, the system automatically:
1. Lowers the relevance threshold
2. Includes more chunks
3. Re-evaluates until confidence is acceptable

This ensures critical information is never dropped.

## Usage

```rust
// Simple — just prints the result
atp_sdk::compress(b"your context data here...", "coding");
// Output: "28.3x compression (50000B → 1768B, 3 chunks, confidence=0.85)"

// Structured — returns CompressResult
let result = atp_sdk::shrink(b"your context data here...", "coding");
println!("Ratio: {}x", result.ratio);
println!("Chunks retained: {}", result.chunks);
println!("Confidence: {}", result.confidence);
```

## Key Functions

```rust
// High-level compressor
pub struct ContextCompressor {
    pub fn new() -> Self
    pub fn with_config(config: MscConfig) -> Self
    pub fn compress_for_task(
        &self, data: &[u8], task_type: TaskType, context: &[u8]
    ) -> Result<ContextDiff, ContextError>
}

// Low-level similarity
pub fn cosine_similarity(a: &ContextEmbedding, b: &ContextEmbedding) -> Result<f64, ContextError>
pub fn batch_cosine_similarity(
    query: &ContextEmbedding, candidates: &[ContextEmbedding]
) -> Result<Vec<(usize, f64)>, ContextError>
```

## Benchmark Impact

From the ablation study:

| Scenario | Cost/Task | Ctx Compression |
|----------|-----------|-----------------|
| ATP (full) | $0.0393 | 28.0x |
| ATP w/o SCD | $0.0627 | 1.0x |

Removing SCD increases cost by **59%** — the single biggest cost driver in the stack.

## Why This Matters

- **Cost**: Sending 28x less context means 28x lower token costs
- **Speed**: Less data to transfer and process
- **Quality**: Irrelevant context can actually hurt agent performance
- **Scalability**: Makes large multi-agent workflows economically viable

## Next Steps

- [[Layer 4: Economic Routing]] — How routes are optimized
- [[Benchmarks]] — Full ablation analysis
