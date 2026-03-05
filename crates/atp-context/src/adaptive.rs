//! Adaptive context negotiation for the SCD layer.
//!
//! When a receiving agent's confidence in the provided context is below
//! the threshold (default: 0.7), it issues a CONTEXT_REQUEST message
//! requesting additional chunks. This module handles:
//!
//! - Confidence evaluation of a received `ContextDiff`
//! - Generation of `ContextRequestMsg` when confidence is insufficient
//! - Processing of incoming context requests to supply additional chunks
//! - Iterative refinement until confidence exceeds the threshold

use atp_types::{
    AgentId, ContextChunk, ContextDiff, ContextEmbedding, ContextError, ContextRequestMsg,
};
use uuid::Uuid;

use crate::differential::ContextCompressor;
use crate::extraction;

/// Default confidence threshold below which a CONTEXT_REQUEST is issued.
pub const DEFAULT_CONFIDENCE_THRESHOLD: f64 = 0.7;

/// Maximum number of adaptive refinement rounds to prevent infinite loops.
pub const MAX_REFINEMENT_ROUNDS: u32 = 5;

/// Adaptive context manager for a receiving agent.
///
/// Evaluates incoming context diffs and generates CONTEXT_REQUEST messages
/// when the confidence is insufficient for task execution.
#[derive(Debug, Clone)]
pub struct AdaptiveContextManager {
    /// Agent ID of this (receiving) agent.
    agent_id: AgentId,
    /// Confidence threshold; below this, request more context.
    confidence_threshold: f64,
    /// Number of refinement rounds performed so far.
    refinement_rounds: u32,
    /// Maximum allowed refinement rounds.
    max_rounds: u32,
    /// Indices of chunks already received, to avoid re-requesting.
    received_indices: std::collections::HashSet<u32>,
}

impl AdaptiveContextManager {
    /// Create a new adaptive context manager for the given agent.
    pub fn new(agent_id: AgentId) -> Self {
        Self {
            agent_id,
            confidence_threshold: DEFAULT_CONFIDENCE_THRESHOLD,
            refinement_rounds: 0,
            max_rounds: MAX_REFINEMENT_ROUNDS,
            received_indices: std::collections::HashSet::new(),
        }
    }

    /// Set a custom confidence threshold.
    pub fn with_threshold(mut self, threshold: f64) -> Self {
        self.confidence_threshold = threshold;
        self
    }

    /// Set the maximum number of refinement rounds.
    pub fn with_max_rounds(mut self, max_rounds: u32) -> Self {
        self.max_rounds = max_rounds;
        self
    }

    /// Get the current confidence threshold.
    pub fn confidence_threshold(&self) -> f64 {
        self.confidence_threshold
    }

    /// Get the number of refinement rounds completed.
    pub fn refinement_rounds(&self) -> u32 {
        self.refinement_rounds
    }

    /// Check whether the received context has sufficient confidence.
    pub fn is_sufficient(&self, diff: &ContextDiff) -> bool {
        diff.confidence >= self.confidence_threshold
    }

    /// Evaluate a received `ContextDiff` and decide whether to request more.
    ///
    /// Returns `Ok(None)` if confidence is sufficient.
    /// Returns `Ok(Some(ContextRequestMsg))` if more context is needed.
    /// Returns `Err(ContextError::LowConfidence)` if max rounds exceeded
    /// and confidence is still below threshold.
    pub fn evaluate(
        &mut self,
        diff: &ContextDiff,
        sender: AgentId,
        task_id: Uuid,
        total_chunks: u32,
    ) -> Result<Option<ContextRequestMsg>, ContextError> {
        // Record the chunk indices we have received.
        for chunk in &diff.chunks {
            self.received_indices.insert(chunk.index);
        }

        // Check if confidence is sufficient.
        if diff.confidence >= self.confidence_threshold {
            return Ok(None);
        }

        // Check if we've exhausted refinement rounds.
        if self.refinement_rounds >= self.max_rounds {
            return Err(ContextError::LowConfidence(diff.confidence));
        }

        self.refinement_rounds += 1;

        // Determine which chunks to request: those we haven't received yet.
        let needed: Vec<u32> = (0..total_chunks)
            .filter(|idx| !self.received_indices.contains(idx))
            .collect();

        if needed.is_empty() {
            // We already have all chunks but confidence is still low.
            return Err(ContextError::LowConfidence(diff.confidence));
        }

        // Request up to a reasonable batch size (e.g., half of remaining).
        let batch_size = needed.len().div_ceil(2).max(1);
        let requested: Vec<u32> = needed.into_iter().take(batch_size).collect();

        Ok(Some(ContextRequestMsg {
            from: self.agent_id,
            to: sender,
            task_id,
            current_confidence: diff.confidence,
            requested_chunk_indices: requested,
        }))
    }

    /// Reset the manager for a new task.
    pub fn reset(&mut self) {
        self.refinement_rounds = 0;
        self.received_indices.clear();
    }
}

/// Context provider: handles incoming CONTEXT_REQUEST messages.
///
/// The sending agent uses this to fulfill requests for additional chunks.
#[derive(Debug)]
pub struct ContextProvider {
    /// The compressor used for chunk extraction.
    compressor: ContextCompressor,
    /// The full original context data.
    full_context: Vec<u8>,
    /// The task embedding for relevance scoring.
    task_embedding: ContextEmbedding,
}

impl ContextProvider {
    /// Create a new context provider with the full context and task embedding.
    pub fn new(
        compressor: ContextCompressor,
        full_context: Vec<u8>,
        task_embedding: ContextEmbedding,
    ) -> Self {
        Self {
            compressor,
            full_context,
            task_embedding,
        }
    }

    /// Handle a CONTEXT_REQUEST by extracting the requested chunks.
    pub fn handle_request(&self, request: &ContextRequestMsg) -> Vec<ContextChunk> {
        self.compressor.extract_additional(
            &self.full_context,
            &self.task_embedding,
            &request.requested_chunk_indices,
        )
    }

    /// Get the total number of chunks in the full context.
    pub fn total_chunks(&self) -> u32 {
        let chunk_size = self.compressor.config().chunk_size;
        if chunk_size == 0 {
            return 0;
        }
        let raw_chunks = extraction::split_into_chunks(&self.full_context, chunk_size);
        raw_chunks.len() as u32
    }

    /// Generate the initial compressed context diff.
    pub fn initial_diff(&self) -> Result<ContextDiff, ContextError> {
        self.compressor.compress(&self.full_context, &self.task_embedding)
    }
}

/// Evaluate whether a CONTEXT_REQUEST should be generated for a received diff.
///
/// Standalone function for simple use cases without the full manager.
pub fn should_request_more(diff: &ContextDiff, threshold: f64) -> bool {
    diff.confidence < threshold
}

/// Generate a simple CONTEXT_REQUEST message.
///
/// Standalone function that requests all chunks not present in the current diff,
/// up to `max_additional` chunks.
pub fn generate_context_request(
    diff: &ContextDiff,
    from: AgentId,
    to: AgentId,
    task_id: Uuid,
    total_chunks: u32,
    max_additional: usize,
) -> Option<ContextRequestMsg> {
    if diff.confidence >= DEFAULT_CONFIDENCE_THRESHOLD {
        return None;
    }

    let received: std::collections::HashSet<u32> =
        diff.chunks.iter().map(|c| c.index).collect();

    let needed: Vec<u32> = (0..total_chunks)
        .filter(|idx| !received.contains(idx))
        .take(max_additional)
        .collect();

    if needed.is_empty() {
        return None;
    }

    Some(ContextRequestMsg {
        from,
        to,
        task_id,
        current_confidence: diff.confidence,
        requested_chunk_indices: needed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::embedding;
    use atp_types::ContextChunk;

    fn make_low_confidence_diff() -> ContextDiff {
        ContextDiff {
            base_hash: [0u8; 32],
            chunks: vec![ContextChunk {
                index: 0,
                data: vec![1, 2, 3],
                relevance_score: 0.5,
            }],
            confidence: 0.5,
            original_size: 5120,
            compressed_size: 3,
        }
    }

    fn make_high_confidence_diff() -> ContextDiff {
        ContextDiff {
            base_hash: [0u8; 32],
            chunks: vec![
                ContextChunk {
                    index: 0,
                    data: vec![1, 2, 3],
                    relevance_score: 0.9,
                },
                ContextChunk {
                    index: 1,
                    data: vec![4, 5, 6],
                    relevance_score: 0.8,
                },
            ],
            confidence: 0.85,
            original_size: 5120,
            compressed_size: 6,
        }
    }

    #[test]
    fn test_should_request_more_low_confidence() {
        let diff = make_low_confidence_diff();
        assert!(should_request_more(&diff, 0.7));
    }

    #[test]
    fn test_should_not_request_more_high_confidence() {
        let diff = make_high_confidence_diff();
        assert!(!should_request_more(&diff, 0.7));
    }

    #[test]
    fn test_adaptive_manager_sufficient() {
        let agent = AgentId::new();
        let sender = AgentId::new();
        let task_id = Uuid::new_v4();

        let mut manager = AdaptiveContextManager::new(agent);
        let diff = make_high_confidence_diff();

        let result = manager.evaluate(&diff, sender, task_id, 10).unwrap();
        assert!(result.is_none(), "should not request more when confidence is high");
    }

    #[test]
    fn test_adaptive_manager_requests_more() {
        let agent = AgentId::new();
        let sender = AgentId::new();
        let task_id = Uuid::new_v4();

        let mut manager = AdaptiveContextManager::new(agent);
        let diff = make_low_confidence_diff();

        let result = manager.evaluate(&diff, sender, task_id, 10).unwrap();
        assert!(result.is_some(), "should request more when confidence is low");

        let request = result.unwrap();
        assert_eq!(request.from, agent);
        assert_eq!(request.to, sender);
        assert_eq!(request.task_id, task_id);
        assert!(!request.requested_chunk_indices.is_empty());
        // Should not request index 0 (already received).
        assert!(!request.requested_chunk_indices.contains(&0));
    }

    #[test]
    fn test_adaptive_manager_max_rounds() {
        let agent = AgentId::new();
        let sender = AgentId::new();
        let task_id = Uuid::new_v4();

        let mut manager = AdaptiveContextManager::new(agent).with_max_rounds(2);
        let diff = make_low_confidence_diff();

        // Round 1: should succeed.
        let r1 = manager.evaluate(&diff, sender, task_id, 10);
        assert!(r1.is_ok());
        assert!(r1.unwrap().is_some());

        // Round 2: should succeed.
        let r2 = manager.evaluate(&diff, sender, task_id, 10);
        assert!(r2.is_ok());
        assert!(r2.unwrap().is_some());

        // Round 3: should fail (max rounds exceeded).
        let r3 = manager.evaluate(&diff, sender, task_id, 10);
        assert!(r3.is_err());
    }

    #[test]
    fn test_adaptive_manager_reset() {
        let agent = AgentId::new();
        let sender = AgentId::new();
        let task_id = Uuid::new_v4();

        let mut manager = AdaptiveContextManager::new(agent).with_max_rounds(1);
        let diff = make_low_confidence_diff();

        // Use up the round.
        let _ = manager.evaluate(&diff, sender, task_id, 10);

        // Reset.
        manager.reset();
        assert_eq!(manager.refinement_rounds(), 0);

        // Should work again after reset.
        let result = manager.evaluate(&diff, sender, task_id, 10);
        assert!(result.is_ok());
    }

    #[test]
    fn test_generate_context_request_none_when_sufficient() {
        let diff = make_high_confidence_diff();
        let result = generate_context_request(
            &diff,
            AgentId::new(),
            AgentId::new(),
            Uuid::new_v4(),
            10,
            5,
        );
        assert!(result.is_none());
    }

    #[test]
    fn test_generate_context_request_some_when_insufficient() {
        let diff = make_low_confidence_diff();
        let from = AgentId::new();
        let to = AgentId::new();
        let task_id = Uuid::new_v4();

        let result = generate_context_request(&diff, from, to, task_id, 10, 3);
        assert!(result.is_some());
        let req = result.unwrap();
        assert!(req.requested_chunk_indices.len() <= 3);
    }

    #[test]
    fn test_context_provider_end_to_end() {
        let dims = 64;
        let data: Vec<u8> = (0..4096).map(|i| (i % 256) as u8).collect();
        let task_emb = embedding::embed(b"provider test task", dims);

        let config = crate::extraction::MscConfig {
            relevance_threshold: -1.0,
            max_chunks: 2,
            chunk_size: 512,
            dimensions: dims,
        };

        let compressor = ContextCompressor::with_config(config);
        let provider = ContextProvider::new(compressor, data, task_emb);

        let total = provider.total_chunks();
        assert_eq!(total, 8); // 4096 / 512

        let diff = provider.initial_diff().unwrap();
        assert!(diff.chunks.len() <= 2);

        // Simulate a context request for chunk index 4.
        let request = ContextRequestMsg {
            from: AgentId::new(),
            to: AgentId::new(),
            task_id: Uuid::new_v4(),
            current_confidence: 0.5,
            requested_chunk_indices: vec![4],
        };

        let additional = provider.handle_request(&request);
        assert_eq!(additional.len(), 1);
        assert_eq!(additional[0].index, 4);
    }

    #[test]
    fn test_custom_threshold() {
        let agent = AgentId::new();
        let manager = AdaptiveContextManager::new(agent).with_threshold(0.9);
        assert!((manager.confidence_threshold() - 0.9).abs() < f64::EPSILON);

        // A diff at 0.85 is insufficient with threshold 0.9.
        let diff = make_high_confidence_diff(); // confidence 0.85
        assert!(!manager.is_sufficient(&diff));
    }
}
