use serde::{Deserialize, Serialize};

/// Dense vector embedding for a task's semantic context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextEmbedding {
    pub dimensions: usize,
    pub values: Vec<f64>,
}

impl ContextEmbedding {
    pub fn new(values: Vec<f64>) -> Self {
        let dimensions = values.len();
        Self { dimensions, values }
    }

    pub fn zeros(dimensions: usize) -> Self {
        Self {
            dimensions,
            values: vec![0.0; dimensions],
        }
    }
}

/// A chunk of context with relevance score.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextChunk {
    pub index: u32,
    pub data: Vec<u8>,
    pub relevance_score: f64,
}

/// A differential context payload -- not the full context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextDiff {
    pub base_hash: [u8; 32],
    pub chunks: Vec<ContextChunk>,
    pub confidence: f64,
    pub original_size: u64,
    pub compressed_size: u64,
}

impl ContextDiff {
    pub fn compression_ratio(&self) -> f64 {
        if self.original_size == 0 {
            return 1.0;
        }
        self.compressed_size as f64 / self.original_size as f64
    }
}
