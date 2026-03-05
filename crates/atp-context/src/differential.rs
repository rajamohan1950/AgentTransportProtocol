//! Context diff generation and application for the SCD layer.
//!
//! Converts MSC extraction results into wire-format `ContextDiff` payloads
//! and supports applying diffs to reconstruct partial context on the
//! receiving side.

use atp_types::{ContextChunk, ContextDiff, ContextEmbedding, ContextError, TaskType};

use crate::embedding;
use crate::extraction::{self, MscConfig, MscResult};

/// The `ContextCompressor` is the primary high-level interface for the SCD layer.
///
/// It takes full context (Vec<u8>), splits into chunks, computes embeddings,
/// compares with a task embedding via cosine similarity, and returns only the
/// chunks above the relevance threshold.
///
/// Target compression: ~28x reduction (original / compressed).
#[derive(Debug, Clone)]
pub struct ContextCompressor {
    /// MSC extraction configuration.
    config: MscConfig,
}

impl ContextCompressor {
    /// Create a new compressor with default settings.
    pub fn new() -> Self {
        Self {
            config: MscConfig::default(),
        }
    }

    /// Create a compressor with custom configuration.
    pub fn with_config(config: MscConfig) -> Self {
        Self { config }
    }

    /// Get a reference to the current configuration.
    pub fn config(&self) -> &MscConfig {
        &self.config
    }

    /// Set the relevance threshold.
    pub fn set_threshold(&mut self, threshold: f64) {
        self.config.relevance_threshold = threshold;
    }

    /// Set the maximum number of chunks (budget).
    pub fn set_budget(&mut self, max_chunks: usize) {
        self.config.max_chunks = max_chunks;
    }

    /// Set the embedding dimensionality.
    pub fn set_dimensions(&mut self, dimensions: usize) {
        self.config.dimensions = dimensions;
    }

    /// Compress full context data into a `ContextDiff` using a task embedding.
    ///
    /// This is the primary entry point:
    /// 1. Splits data into chunks
    /// 2. Embeds each chunk
    /// 3. Computes cosine similarity against the task embedding
    /// 4. Retains only chunks above the threshold
    /// 5. Returns a wire-format `ContextDiff`
    ///
    /// # Errors
    /// Returns `ContextError` if extraction fails or dimensions mismatch.
    pub fn compress(
        &self,
        data: &[u8],
        task_embedding: &ContextEmbedding,
    ) -> Result<ContextDiff, ContextError> {
        let msc = extraction::extract_msc(data, task_embedding, &self.config)?;
        Ok(msc_to_diff(msc))
    }

    /// Convenience method: embed the task description and then compress.
    pub fn compress_for_task(
        &self,
        data: &[u8],
        task_type: TaskType,
        task_description: &[u8],
    ) -> Result<ContextDiff, ContextError> {
        let task_emb = embedding::embed_task(task_type, task_description, self.config.dimensions);
        self.compress(data, &task_emb)
    }

    /// Extract additional chunks by index (for CONTEXT_REQUEST responses).
    pub fn extract_additional(
        &self,
        data: &[u8],
        task_embedding: &ContextEmbedding,
        indices: &[u32],
    ) -> Vec<ContextChunk> {
        extraction::extract_chunks_by_index(
            data,
            self.config.chunk_size,
            indices,
            self.config.dimensions,
            task_embedding,
        )
    }
}

impl Default for ContextCompressor {
    fn default() -> Self {
        Self::new()
    }
}

/// Convert an `MscResult` into a wire-format `ContextDiff`.
pub fn msc_to_diff(msc: MscResult) -> ContextDiff {
    ContextDiff {
        base_hash: msc.base_hash,
        chunks: msc.chunks,
        confidence: msc.confidence,
        original_size: msc.original_size as u64,
        compressed_size: msc.compressed_size as u64,
    }
}

/// Apply a `ContextDiff` to reconstruct partial context.
///
/// Returns the concatenated data of all chunks in index order.
/// This is a partial reconstruction -- only the selected chunks are present.
pub fn apply_diff(diff: &ContextDiff) -> Vec<u8> {
    let mut sorted_chunks: Vec<&ContextChunk> = diff.chunks.iter().collect();
    sorted_chunks.sort_by_key(|c| c.index);

    let total_size: usize = sorted_chunks.iter().map(|c| c.data.len()).sum();
    let mut result = Vec::with_capacity(total_size);
    for chunk in sorted_chunks {
        result.extend_from_slice(&chunk.data);
    }
    result
}

/// Merge additional chunks into an existing `ContextDiff`.
///
/// Deduplicates by chunk index and re-sorts. Recalculates confidence
/// and compressed size.
pub fn merge_chunks(diff: &mut ContextDiff, additional: Vec<ContextChunk>) {
    // Collect existing indices for dedup.
    let existing_indices: std::collections::HashSet<u32> =
        diff.chunks.iter().map(|c| c.index).collect();

    for chunk in additional {
        if !existing_indices.contains(&chunk.index) {
            diff.chunks.push(chunk);
        }
    }

    // Re-sort by relevance descending.
    diff.chunks
        .sort_by(|a, b| b.relevance_score.partial_cmp(&a.relevance_score).unwrap_or(std::cmp::Ordering::Equal));

    // Recalculate metadata.
    diff.compressed_size = diff.chunks.iter().map(|c| c.data.len() as u64).sum();
    if !diff.chunks.is_empty() {
        let total: f64 = diff.chunks.iter().map(|c| c.relevance_score).sum();
        diff.confidence = total / diff.chunks.len() as f64;
    }
}

/// Compute statistics about a `ContextDiff` for diagnostics.
#[derive(Debug, Clone)]
pub struct DiffStats {
    pub num_chunks: usize,
    pub original_size: u64,
    pub compressed_size: u64,
    pub compression_ratio: f64,
    pub avg_relevance: f64,
    pub min_relevance: f64,
    pub max_relevance: f64,
    pub confidence: f64,
}

/// Compute statistics for a context diff.
pub fn diff_stats(diff: &ContextDiff) -> DiffStats {
    let num_chunks = diff.chunks.len();
    let (min_rel, max_rel, sum_rel) = if num_chunks == 0 {
        (0.0, 0.0, 0.0)
    } else {
        diff.chunks.iter().fold(
            (f64::MAX, f64::MIN, 0.0_f64),
            |(min, max, sum), c| {
                (
                    min.min(c.relevance_score),
                    max.max(c.relevance_score),
                    sum + c.relevance_score,
                )
            },
        )
    };

    let avg_rel = if num_chunks > 0 {
        sum_rel / num_chunks as f64
    } else {
        0.0
    };

    DiffStats {
        num_chunks,
        original_size: diff.original_size,
        compressed_size: diff.compressed_size,
        compression_ratio: diff.compression_ratio(),
        avg_relevance: avg_rel,
        min_relevance: if num_chunks > 0 { min_rel } else { 0.0 },
        max_relevance: if num_chunks > 0 { max_rel } else { 0.0 },
        confidence: diff.confidence,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::embedding;

    fn make_test_data(size: usize) -> Vec<u8> {
        (0..size).map(|i| (i % 256) as u8).collect()
    }

    #[test]
    fn test_compressor_basic() {
        let dims = 64;
        let data = make_test_data(8192);
        let task_emb = embedding::embed(b"test task for compression", dims);

        let mut compressor = ContextCompressor::new();
        compressor.set_dimensions(dims);
        compressor.set_threshold(-1.0); // accept all for test

        let diff = compressor.compress(&data, &task_emb).unwrap();
        assert_eq!(diff.original_size, 8192);
        assert!(!diff.chunks.is_empty());
    }

    #[test]
    fn test_compressor_with_budget() {
        let dims = 64;
        let data = make_test_data(8192);
        let task_emb = embedding::embed(b"budget test", dims);

        let mut compressor = ContextCompressor::new();
        compressor.set_dimensions(dims);
        compressor.set_threshold(-1.0);
        compressor.set_budget(2);

        let diff = compressor.compress(&data, &task_emb).unwrap();
        assert!(diff.chunks.len() <= 2);
    }

    #[test]
    fn test_compressor_for_task() {
        let dims = 64;
        let data = make_test_data(4096);

        let mut compressor = ContextCompressor::new();
        compressor.set_dimensions(dims);
        compressor.set_threshold(-1.0);

        let diff = compressor
            .compress_for_task(&data, TaskType::CodeGeneration, b"parse JSON files")
            .unwrap();
        assert_eq!(diff.original_size, 4096);
        assert!(!diff.chunks.is_empty());
    }

    #[test]
    fn test_apply_diff_order() {
        // Create a diff with chunks out of order.
        let diff = ContextDiff {
            base_hash: [0u8; 32],
            chunks: vec![
                ContextChunk {
                    index: 2,
                    data: vec![3, 4],
                    relevance_score: 0.5,
                },
                ContextChunk {
                    index: 0,
                    data: vec![1, 2],
                    relevance_score: 0.9,
                },
            ],
            confidence: 0.7,
            original_size: 100,
            compressed_size: 4,
        };

        let reconstructed = apply_diff(&diff);
        assert_eq!(reconstructed, vec![1, 2, 3, 4]);
    }

    #[test]
    fn test_merge_chunks_dedup() {
        let mut diff = ContextDiff {
            base_hash: [0u8; 32],
            chunks: vec![ContextChunk {
                index: 0,
                data: vec![1],
                relevance_score: 0.8,
            }],
            confidence: 0.8,
            original_size: 100,
            compressed_size: 1,
        };

        let additional = vec![
            ContextChunk {
                index: 0, // duplicate
                data: vec![1],
                relevance_score: 0.8,
            },
            ContextChunk {
                index: 1, // new
                data: vec![2],
                relevance_score: 0.6,
            },
        ];

        merge_chunks(&mut diff, additional);
        assert_eq!(diff.chunks.len(), 2);
    }

    #[test]
    fn test_diff_stats() {
        let diff = ContextDiff {
            base_hash: [0u8; 32],
            chunks: vec![
                ContextChunk {
                    index: 0,
                    data: vec![0; 512],
                    relevance_score: 0.9,
                },
                ContextChunk {
                    index: 1,
                    data: vec![0; 512],
                    relevance_score: 0.7,
                },
            ],
            confidence: 0.8,
            original_size: 51200,
            compressed_size: 1024,
        };

        let stats = diff_stats(&diff);
        assert_eq!(stats.num_chunks, 2);
        assert!((stats.avg_relevance - 0.8).abs() < 1e-10);
        assert!((stats.min_relevance - 0.7).abs() < 1e-10);
        assert!((stats.max_relevance - 0.9).abs() < 1e-10);
        assert!((stats.compression_ratio - 0.02).abs() < 0.001);
    }

    #[test]
    fn test_compression_achieves_reduction() {
        // With a reasonable threshold, we should see significant reduction.
        let dims = 64;
        let data = make_test_data(16384); // 16 KB
        let task_emb = embedding::embed(b"specific narrow task", dims);

        let mut compressor = ContextCompressor::new();
        compressor.set_dimensions(dims);
        compressor.set_threshold(0.3);

        let diff = compressor.compress(&data, &task_emb).unwrap();
        let ratio = diff.original_size as f64 / diff.compressed_size.max(1) as f64;

        // We expect meaningful compression (at least some chunks filtered).
        assert!(
            diff.compressed_size < diff.original_size,
            "compressed ({}) should be smaller than original ({})",
            diff.compressed_size,
            diff.original_size,
        );

        // Log for diagnostics.
        eprintln!(
            "Compression: {} -> {} bytes (ratio: {:.1}x, chunks: {}/{})",
            diff.original_size,
            diff.compressed_size,
            ratio,
            diff.chunks.len(),
            data.len() / 512,
        );
    }

    #[test]
    fn test_extract_additional() {
        let dims = 64;
        let data = make_test_data(4096);
        let task_emb = embedding::embed(b"additional test", dims);

        let mut compressor = ContextCompressor::new();
        compressor.set_dimensions(dims);

        let additional = compressor.extract_additional(&data, &task_emb, &[1, 3, 5]);
        assert_eq!(additional.len(), 3);
        assert_eq!(additional[0].index, 1);
        assert_eq!(additional[1].index, 3);
        assert_eq!(additional[2].index, 5);
    }
}
