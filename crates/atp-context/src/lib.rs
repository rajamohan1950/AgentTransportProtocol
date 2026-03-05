//! # ATP Context: Semantic Context Differentials (Layer 3)
//!
//! This crate implements the Semantic Context Differential (SCD) layer of the
//! Agent Transport Protocol. The SCD layer reduces context transfer between
//! agents by 60-80% through intelligent context compression.
//!
//! ## Architecture
//!
//! The SCD pipeline operates in four stages:
//!
//! 1. **Task Embedding** (`embedding`) -- Dense vector representation of the
//!    task using hash-based simulation embeddings. Each task and context chunk
//!    is mapped to a high-dimensional vector space.
//!
//! 2. **Cosine Similarity** (`similarity`) -- Measures semantic relevance
//!    between task and context chunk embeddings using cosine similarity:
//!    `cos(a, b) = (a . b) / (|a| * |b|)`
//!
//! 3. **Minimal Sufficient Context Extraction** (`extraction`) -- Selects only
//!    the context chunks whose similarity to the task embedding exceeds a
//!    threshold:
//!    `MSC = {(chunk, score) : cosine(e_task, e_chunk) > threshold}`
//!    Chunks are ranked by relevance and truncated to the receiver's budget.
//!
//! 4. **Context Diff Generation** (`differential`) -- Packages the selected
//!    chunks into a wire-format `ContextDiff` for transmission. The
//!    `ContextCompressor` is the high-level entry point.
//!
//! 5. **Adaptive Context** (`adaptive`) -- When the receiving agent's
//!    confidence in the provided context is below 0.7, it issues a
//!    `CONTEXT_REQUEST` for additional chunks. This iterative refinement
//!    ensures task quality without transmitting the full context.
//!
//! ## Quick Start
//!
//! ```rust
//! use atp_context::differential::ContextCompressor;
//! use atp_context::extraction::MscConfig;
//! use atp_context::embedding;
//! use atp_types::TaskType;
//!
//! // Create a compressor with custom settings.
//! let config = MscConfig {
//!     relevance_threshold: 0.3,
//!     max_chunks: 10,
//!     chunk_size: 512,
//!     dimensions: 64,
//! };
//! let compressor = ContextCompressor::with_config(config);
//!
//! // Compress context for a code generation task.
//! let full_context = b"large context data here...".to_vec();
//! let diff = compressor
//!     .compress_for_task(&full_context, TaskType::CodeGeneration, b"parse JSON")
//!     .unwrap();
//!
//! println!("Compression: {}x", diff.original_size as f64 / diff.compressed_size.max(1) as f64);
//! ```

pub mod adaptive;
pub mod differential;
pub mod embedding;
pub mod extraction;
pub mod similarity;

// Re-export primary types for convenience.
pub use adaptive::{AdaptiveContextManager, ContextProvider};
pub use differential::{ContextCompressor, DiffStats};
pub use extraction::{MscConfig, MscResult};
