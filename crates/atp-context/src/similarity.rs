//! Cosine similarity computation for ATP context embeddings.
//!
//! Cosine similarity measures the angle between two vectors, yielding a
//! value in [-1, 1] where 1 means identical direction, 0 means orthogonal,
//! and -1 means opposite direction. For normalized embeddings (unit vectors),
//! cosine similarity reduces to the dot product.

use atp_types::{ContextEmbedding, ContextError};
use ndarray::Array1;

use crate::embedding;

/// Compute cosine similarity between two embedding vectors.
///
/// Returns the cosine of the angle between the vectors:
///   cos(a, b) = (a . b) / (|a| * |b|)
///
/// Returns 0.0 if either vector has zero magnitude.
///
/// # Errors
/// Returns `ContextError::DimensionMismatch` if dimensions differ.
pub fn cosine_similarity(
    a: &ContextEmbedding,
    b: &ContextEmbedding,
) -> Result<f64, ContextError> {
    embedding::validate_dimensions(a, b)?;

    let va = embedding::to_array(a);
    let vb = embedding::to_array(b);

    Ok(cosine_similarity_arrays(&va, &vb))
}

/// Compute cosine similarity between two ndarray vectors directly.
/// Returns 0.0 if either vector has zero magnitude.
///
/// # Panics
/// Panics if the arrays have different lengths (this is an internal function;
/// dimension validation should happen at the ContextEmbedding level).
pub fn cosine_similarity_arrays(a: &Array1<f64>, b: &Array1<f64>) -> f64 {
    debug_assert_eq!(a.len(), b.len(), "array dimension mismatch");

    let dot = a.dot(b);
    let norm_a = embedding::l2_norm(a);
    let norm_b = embedding::l2_norm(b);

    let denom = norm_a * norm_b;
    if denom < f64::EPSILON {
        return 0.0;
    }

    // Clamp to [-1, 1] to handle floating-point drift.
    (dot / denom).clamp(-1.0, 1.0)
}

/// Batch computation: compute cosine similarity of a query embedding against
/// multiple candidate embeddings. Returns a vector of (index, similarity)
/// pairs sorted by similarity descending.
///
/// # Errors
/// Returns `ContextError::DimensionMismatch` if any candidate has different
/// dimensions than the query.
pub fn rank_by_similarity(
    query: &ContextEmbedding,
    candidates: &[ContextEmbedding],
) -> Result<Vec<(usize, f64)>, ContextError> {
    let query_arr = embedding::to_array(query);
    let query_norm = embedding::l2_norm(&query_arr);

    if query_norm < f64::EPSILON {
        // Zero query: all similarities are 0.
        return Ok(candidates.iter().enumerate().map(|(i, _)| (i, 0.0)).collect());
    }

    let query_normed = &query_arr / query_norm;

    let mut scored: Vec<(usize, f64)> = candidates
        .iter()
        .enumerate()
        .map(|(i, c)| {
            if c.dimensions != query.dimensions {
                Err(ContextError::DimensionMismatch {
                    expected: query.dimensions,
                    got: c.dimensions,
                })
            } else {
                let ca = embedding::to_array(c);
                let cn = embedding::l2_norm(&ca);
                if cn < f64::EPSILON {
                    Ok((i, 0.0))
                } else {
                    let sim = query_normed.dot(&(&ca / cn)).clamp(-1.0, 1.0);
                    Ok((i, sim))
                }
            }
        })
        .collect::<Result<Vec<_>, _>>()?;

    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    Ok(scored)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::embedding::embed;

    #[test]
    fn test_identical_vectors_similarity_one() {
        let emb = embed(b"identical", 64);
        let sim = cosine_similarity(&emb, &emb).unwrap();
        assert!(
            (sim - 1.0).abs() < 1e-10,
            "identical vectors should have similarity 1.0, got {sim}"
        );
    }

    #[test]
    fn test_orthogonal_vectors() {
        // Construct two orthogonal unit vectors manually.
        let mut a_vals = vec![0.0; 64];
        let mut b_vals = vec![0.0; 64];
        a_vals[0] = 1.0;
        b_vals[1] = 1.0;
        let a = ContextEmbedding::new(a_vals);
        let b = ContextEmbedding::new(b_vals);

        let sim = cosine_similarity(&a, &b).unwrap();
        assert!(sim.abs() < 1e-10, "orthogonal vectors should be ~0, got {sim}");
    }

    #[test]
    fn test_opposite_vectors() {
        let vals: Vec<f64> = (0..32).map(|i| (i as f64) * 0.1).collect();
        let neg_vals: Vec<f64> = vals.iter().map(|&v| -v).collect();
        let a = ContextEmbedding::new(vals);
        let b = ContextEmbedding::new(neg_vals);

        let sim = cosine_similarity(&a, &b).unwrap();
        assert!(
            (sim + 1.0).abs() < 1e-10,
            "opposite vectors should have similarity -1.0, got {sim}"
        );
    }

    #[test]
    fn test_dimension_mismatch_error() {
        let a = ContextEmbedding::zeros(32);
        let b = ContextEmbedding::zeros(64);
        let result = cosine_similarity(&a, &b);
        assert!(result.is_err());
    }

    #[test]
    fn test_zero_vector_similarity() {
        let a = ContextEmbedding::zeros(16);
        let b = embed(b"nonzero", 16);
        let sim = cosine_similarity(&a, &b).unwrap();
        assert_eq!(sim, 0.0);
    }

    #[test]
    fn test_rank_by_similarity_ordering() {
        let query = embed(b"target query", 64);
        let close = embed(b"target query nearby", 64);
        let far = embed(b"completely unrelated data XYZ 12345 !@#$%", 64);

        let candidates = vec![far.clone(), close.clone()];
        let ranked = rank_by_similarity(&query, &candidates).unwrap();

        // The first result should have the highest similarity.
        assert!(ranked[0].1 >= ranked[1].1);
    }

    #[test]
    fn test_similarity_symmetry() {
        let a = embed(b"first", 128);
        let b = embed(b"second", 128);
        let sim_ab = cosine_similarity(&a, &b).unwrap();
        let sim_ba = cosine_similarity(&b, &a).unwrap();
        assert!((sim_ab - sim_ba).abs() < 1e-15);
    }
}
