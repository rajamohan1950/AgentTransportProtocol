//! Dense vector embedding operations for ATP Semantic Context Differentials.
//!
//! Provides hash-based embedding generation for simulation purposes and
//! vector operations using ndarray for efficient computation.

use atp_types::{ContextEmbedding, ContextError, TaskType};
use ndarray::Array1;
use sha2::{Digest, Sha256};

/// Default embedding dimension matching the ContextConfig default.
pub const DEFAULT_DIMENSIONS: usize = 768;

/// Convert a `ContextEmbedding` into an ndarray `Array1<f64>` for efficient
/// vector operations.
pub fn to_array(embedding: &ContextEmbedding) -> Array1<f64> {
    Array1::from_vec(embedding.values.clone())
}

/// Convert an ndarray `Array1<f64>` back into a `ContextEmbedding`.
pub fn from_array(array: &Array1<f64>) -> ContextEmbedding {
    ContextEmbedding::new(array.to_vec())
}

/// Normalize a vector to unit length (L2 norm).
/// Returns a zero vector if the input has zero magnitude.
pub fn normalize(v: &Array1<f64>) -> Array1<f64> {
    let norm = v.dot(v).sqrt();
    if norm < f64::EPSILON {
        Array1::zeros(v.len())
    } else {
        v / norm
    }
}

/// Compute the L2 norm (magnitude) of a vector.
pub fn l2_norm(v: &Array1<f64>) -> f64 {
    v.dot(v).sqrt()
}

/// Generate a deterministic embedding from arbitrary byte data using SHA-256
/// hash-based expansion. This is a simulation embedding -- real deployments
/// would use a learned encoder.
///
/// The approach: hash the data, then iteratively re-hash to fill
/// `dimensions` floats in the range [-1, 1].
pub fn embed(data: &[u8], dimensions: usize) -> ContextEmbedding {
    let mut values = Vec::with_capacity(dimensions);
    let mut hasher = Sha256::new();
    hasher.update(data);
    let mut current_hash = hasher.finalize_reset();

    let mut i = 0;
    while i < dimensions {
        // Each SHA-256 hash gives 32 bytes = 4 f64 values (8 bytes each).
        // We interpret each 8-byte block as a u64, then map to [-1, 1].
        for chunk_start in (0..32).step_by(8) {
            if i >= dimensions {
                break;
            }
            let bytes: [u8; 8] = current_hash[chunk_start..chunk_start + 8]
                .try_into()
                .expect("slice is exactly 8 bytes");
            let raw = u64::from_le_bytes(bytes);
            // Map u64 to [-1.0, 1.0]
            let val = (raw as f64 / u64::MAX as f64) * 2.0 - 1.0;
            values.push(val);
            i += 1;
        }
        // Re-hash for next round of values
        hasher.update(current_hash);
        current_hash = hasher.finalize_reset();
    }

    // Normalize the resulting vector to unit length
    let array = Array1::from_vec(values);
    let normed = normalize(&array);
    ContextEmbedding::new(normed.to_vec())
}

/// Generate a task embedding based on the task type and optional description.
/// The task type adds a deterministic seed so that same-type tasks cluster
/// together in the embedding space.
pub fn embed_task(task_type: TaskType, description: &[u8], dimensions: usize) -> ContextEmbedding {
    let type_tag = match task_type {
        TaskType::CodeGeneration => b"atp:task:code_generation:" as &[u8],
        TaskType::Analysis => b"atp:task:analysis:",
        TaskType::CreativeWriting => b"atp:task:creative_writing:",
        TaskType::DataProcessing => b"atp:task:data_processing:",
    };

    let mut tagged = Vec::with_capacity(type_tag.len() + description.len());
    tagged.extend_from_slice(type_tag);
    tagged.extend_from_slice(description);

    embed(&tagged, dimensions)
}

/// Validate that two embeddings have matching dimensions.
pub fn validate_dimensions(
    a: &ContextEmbedding,
    b: &ContextEmbedding,
) -> Result<(), ContextError> {
    if a.dimensions != b.dimensions {
        return Err(ContextError::DimensionMismatch {
            expected: a.dimensions,
            got: b.dimensions,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_embed_produces_correct_dimensions() {
        let emb = embed(b"hello world", 128);
        assert_eq!(emb.dimensions, 128);
        assert_eq!(emb.values.len(), 128);
    }

    #[test]
    fn test_embed_produces_unit_vector() {
        let emb = embed(b"test data", DEFAULT_DIMENSIONS);
        let arr = to_array(&emb);
        let norm = l2_norm(&arr);
        assert!((norm - 1.0).abs() < 1e-10, "norm = {norm}");
    }

    #[test]
    fn test_embed_is_deterministic() {
        let a = embed(b"deterministic test", 64);
        let b = embed(b"deterministic test", 64);
        assert_eq!(a.values, b.values);
    }

    #[test]
    fn test_embed_different_data_different_vectors() {
        let a = embed(b"data A", 64);
        let b = embed(b"data B", 64);
        assert_ne!(a.values, b.values);
    }

    #[test]
    fn test_normalize_zero_vector() {
        let zero = Array1::zeros(10);
        let result = normalize(&zero);
        assert!(result.iter().all(|&v| v == 0.0));
    }

    #[test]
    fn test_embed_task_different_types_differ() {
        let code = embed_task(TaskType::CodeGeneration, b"parse JSON", 128);
        let analysis = embed_task(TaskType::Analysis, b"parse JSON", 128);
        assert_ne!(code.values, analysis.values);
    }

    #[test]
    fn test_validate_dimensions_match() {
        let a = ContextEmbedding::zeros(64);
        let b = ContextEmbedding::zeros(64);
        assert!(validate_dimensions(&a, &b).is_ok());
    }

    #[test]
    fn test_validate_dimensions_mismatch() {
        let a = ContextEmbedding::zeros(64);
        let b = ContextEmbedding::zeros(128);
        assert!(validate_dimensions(&a, &b).is_err());
    }

    #[test]
    fn test_roundtrip_array_conversion() {
        let emb = embed(b"roundtrip", 32);
        let arr = to_array(&emb);
        let back = from_array(&arr);
        assert_eq!(emb.values, back.values);
        assert_eq!(emb.dimensions, back.dimensions);
    }
}
