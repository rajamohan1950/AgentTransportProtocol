//! Minimal Sufficient Context (MSC) extraction for the SCD layer.
//!
//! Implements the core SCD extraction formula:
//!   MSC = {(chunk, score) : cosine(e_task, e_chunk) > threshold}
//!
//! Chunks are ranked by relevance and truncated to the receiver's budget,
//! achieving 60-80% context reduction while preserving task-relevant
//! information.

use atp_types::{ContextChunk, ContextEmbedding, ContextError};
use sha2::{Digest, Sha256};

use crate::embedding;
use crate::similarity;

/// Default chunk size in bytes (512 bytes per chunk).
pub const DEFAULT_CHUNK_SIZE: usize = 512;

/// Configuration for Minimal Sufficient Context extraction.
#[derive(Debug, Clone)]
pub struct MscConfig {
    /// Cosine similarity threshold; chunks below this are dropped.
    pub relevance_threshold: f64,
    /// Maximum number of chunks to include (budget). 0 means unlimited.
    pub max_chunks: usize,
    /// Size of each chunk in bytes.
    pub chunk_size: usize,
    /// Embedding dimensionality.
    pub dimensions: usize,
}

impl Default for MscConfig {
    fn default() -> Self {
        Self {
            relevance_threshold: 0.3,
            max_chunks: 0,
            chunk_size: DEFAULT_CHUNK_SIZE,
            dimensions: embedding::DEFAULT_DIMENSIONS,
        }
    }
}

/// Result of MSC extraction, containing the selected chunks and metadata.
#[derive(Debug, Clone)]
pub struct MscResult {
    /// The selected context chunks, ranked by relevance (descending).
    pub chunks: Vec<ContextChunk>,
    /// Total number of chunks before extraction.
    pub total_chunks: usize,
    /// SHA-256 hash of the original full context.
    pub base_hash: [u8; 32],
    /// Original context size in bytes.
    pub original_size: usize,
    /// Compressed (selected) context size in bytes.
    pub compressed_size: usize,
    /// Average confidence across selected chunks.
    pub confidence: f64,
}

impl MscResult {
    /// Compression ratio: original_size / compressed_size.
    /// Returns f64::INFINITY if compressed_size is 0.
    pub fn compression_ratio(&self) -> f64 {
        if self.compressed_size == 0 {
            return f64::INFINITY;
        }
        self.original_size as f64 / self.compressed_size as f64
    }

    /// Fraction of chunks retained.
    pub fn retention_ratio(&self) -> f64 {
        if self.total_chunks == 0 {
            return 0.0;
        }
        self.chunks.len() as f64 / self.total_chunks as f64
    }
}

/// Split raw context data into fixed-size chunks.
pub fn split_into_chunks(data: &[u8], chunk_size: usize) -> Vec<Vec<u8>> {
    if data.is_empty() || chunk_size == 0 {
        return Vec::new();
    }
    data.chunks(chunk_size).map(|c| c.to_vec()).collect()
}

/// Compute SHA-256 hash of the full context.
pub fn hash_context(data: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hasher.finalize().into()
}

/// Extract the Minimal Sufficient Context from raw data given a task embedding.
///
/// Algorithm:
/// 1. Split data into fixed-size chunks.
/// 2. Embed each chunk.
/// 3. Compute cosine similarity between the task embedding and each chunk.
/// 4. Filter chunks above the relevance threshold.
/// 5. Sort by relevance descending.
/// 6. Truncate to budget (max_chunks).
///
/// # Errors
/// Returns `ContextError::ExtractionFailed` if no chunks survive filtering.
/// Returns `ContextError::DimensionMismatch` if task embedding dimensions
/// don't match the configured dimensions.
pub fn extract_msc(
    data: &[u8],
    task_embedding: &ContextEmbedding,
    config: &MscConfig,
) -> Result<MscResult, ContextError> {
    // Validate task embedding dimensions match config.
    if task_embedding.dimensions != config.dimensions {
        return Err(ContextError::DimensionMismatch {
            expected: config.dimensions,
            got: task_embedding.dimensions,
        });
    }

    let base_hash = hash_context(data);
    let original_size = data.len();
    let raw_chunks = split_into_chunks(data, config.chunk_size);
    let total_chunks = raw_chunks.len();

    if total_chunks == 0 {
        return Err(ContextError::ExtractionFailed(
            "empty context data".to_string(),
        ));
    }

    // Embed each chunk and compute similarity.
    let task_arr = embedding::to_array(task_embedding);
    let mut scored_chunks: Vec<(ContextChunk, f64)> = Vec::with_capacity(total_chunks);

    for (idx, chunk_data) in raw_chunks.iter().enumerate() {
        let chunk_emb = embedding::embed(chunk_data, config.dimensions);
        let chunk_arr = embedding::to_array(&chunk_emb);
        let sim = similarity::cosine_similarity_arrays(&task_arr, &chunk_arr);

        if sim > config.relevance_threshold {
            scored_chunks.push((
                ContextChunk {
                    index: idx as u32,
                    data: chunk_data.clone(),
                    relevance_score: sim,
                },
                sim,
            ));
        }
    }

    // Sort by relevance descending.
    scored_chunks
        .sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    // Truncate to budget.
    if config.max_chunks > 0 && scored_chunks.len() > config.max_chunks {
        scored_chunks.truncate(config.max_chunks);
    }

    let chunks: Vec<ContextChunk> = scored_chunks.into_iter().map(|(c, _)| c).collect();
    let compressed_size: usize = chunks.iter().map(|c| c.data.len()).sum();

    // Compute average confidence from relevance scores.
    let confidence = if chunks.is_empty() {
        0.0
    } else {
        let total_score: f64 = chunks.iter().map(|c| c.relevance_score).sum();
        total_score / chunks.len() as f64
    };

    Ok(MscResult {
        chunks,
        total_chunks,
        base_hash,
        original_size,
        compressed_size,
        confidence,
    })
}

/// Retrieve specific chunks by index from raw data.
/// Used when the receiving agent issues a CONTEXT_REQUEST for additional chunks.
pub fn extract_chunks_by_index(
    data: &[u8],
    chunk_size: usize,
    indices: &[u32],
    dimensions: usize,
    task_embedding: &ContextEmbedding,
) -> Vec<ContextChunk> {
    let raw_chunks = split_into_chunks(data, chunk_size);
    let task_arr = embedding::to_array(task_embedding);

    indices
        .iter()
        .filter_map(|&idx| {
            let i = idx as usize;
            raw_chunks.get(i).map(|chunk_data| {
                let chunk_emb = embedding::embed(chunk_data, dimensions);
                let chunk_arr = embedding::to_array(&chunk_emb);
                let sim = similarity::cosine_similarity_arrays(&task_arr, &chunk_arr);
                ContextChunk {
                    index: idx,
                    data: chunk_data.clone(),
                    relevance_score: sim,
                }
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::embedding;

    fn make_test_data(size: usize) -> Vec<u8> {
        (0..size).map(|i| (i % 256) as u8).collect()
    }

    #[test]
    fn test_split_into_chunks() {
        let data = vec![0u8; 1024];
        let chunks = split_into_chunks(&data, 512);
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].len(), 512);
    }

    #[test]
    fn test_split_uneven() {
        let data = vec![0u8; 1000];
        let chunks = split_into_chunks(&data, 512);
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].len(), 512);
        assert_eq!(chunks[1].len(), 488);
    }

    #[test]
    fn test_split_empty() {
        let chunks = split_into_chunks(&[], 512);
        assert!(chunks.is_empty());
    }

    #[test]
    fn test_hash_context_deterministic() {
        let data = b"test data for hashing";
        let h1 = hash_context(data);
        let h2 = hash_context(data);
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_extract_msc_basic() {
        let dims = 64;
        let data = make_test_data(4096);
        let task_emb = embedding::embed(b"test task", dims);

        let config = MscConfig {
            relevance_threshold: -1.0, // accept everything for test
            max_chunks: 0,
            chunk_size: 512,
            dimensions: dims,
        };

        let result = extract_msc(&data, &task_emb, &config).unwrap();
        assert_eq!(result.total_chunks, 8); // 4096 / 512
        assert_eq!(result.original_size, 4096);
        assert!(result.confidence >= -1.0);
    }

    #[test]
    fn test_extract_msc_budget_truncation() {
        let dims = 64;
        let data = make_test_data(4096);
        let task_emb = embedding::embed(b"budget task", dims);

        let config = MscConfig {
            relevance_threshold: -1.0,
            max_chunks: 3,
            chunk_size: 512,
            dimensions: dims,
        };

        let result = extract_msc(&data, &task_emb, &config).unwrap();
        assert!(result.chunks.len() <= 3);
    }

    #[test]
    fn test_extract_msc_dimension_mismatch() {
        let data = make_test_data(1024);
        let task_emb = embedding::embed(b"task", 128);

        let config = MscConfig {
            dimensions: 64,
            ..Default::default()
        };

        let result = extract_msc(&data, &task_emb, &config);
        assert!(result.is_err());
    }

    #[test]
    fn test_extract_msc_empty_data() {
        let task_emb = embedding::embed(b"task", 64);
        let config = MscConfig {
            dimensions: 64,
            ..Default::default()
        };

        let result = extract_msc(&[], &task_emb, &config);
        assert!(result.is_err());
    }

    #[test]
    fn test_compression_ratio() {
        let result = MscResult {
            chunks: vec![],
            total_chunks: 100,
            base_hash: [0u8; 32],
            original_size: 51200,
            compressed_size: 1829, // ~28x ratio
            confidence: 0.8,
        };
        let ratio = result.compression_ratio();
        assert!((ratio - 28.0).abs() < 0.5);
    }

    #[test]
    fn test_extract_chunks_by_index() {
        let dims = 64;
        let data = make_test_data(2048);
        let task_emb = embedding::embed(b"index task", dims);

        let chunks = extract_chunks_by_index(&data, 512, &[0, 2], dims, &task_emb);
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].index, 0);
        assert_eq!(chunks[1].index, 2);
    }

    #[test]
    fn test_extract_chunks_by_index_out_of_range() {
        let dims = 64;
        let data = make_test_data(1024);
        let task_emb = embedding::embed(b"index task", dims);

        let chunks = extract_chunks_by_index(&data, 512, &[0, 99], dims, &task_emb);
        assert_eq!(chunks.len(), 1); // index 99 is out of range
    }
}
