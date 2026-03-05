use pyo3::prelude::*;
use atp_context::extraction::MscConfig;
use crate::py_types::*;

/// Context compression result.
#[pyclass]
#[derive(Clone)]
pub struct PyCompressionResult {
    #[pyo3(get)]
    pub original_size: u64,
    #[pyo3(get)]
    pub compressed_size: u64,
    #[pyo3(get)]
    pub compression_ratio: f64,
    #[pyo3(get)]
    pub chunks_kept: usize,
    #[pyo3(get)]
    pub total_chunks: usize,
    #[pyo3(get)]
    pub confidence: f64,
}

#[pymethods]
impl PyCompressionResult {
    fn __repr__(&self) -> String {
        format!(
            "CompressionResult(ratio={:.1}x, chunks={}/{}, confidence={:.2})",
            self.compression_ratio, self.chunks_kept, self.total_chunks, self.confidence
        )
    }
}

/// Semantic Context Differential compressor.
#[pyclass]
pub struct PyContextCompressor {
    inner: atp_context::ContextCompressor,
    chunk_size: usize,
}

#[pymethods]
impl PyContextCompressor {
    /// Create a new compressor with default settings.
    #[new]
    #[pyo3(signature = (relevance_threshold=0.3, chunk_size=512, dimensions=256))]
    fn new(relevance_threshold: f64, chunk_size: usize, dimensions: usize) -> Self {
        let config = MscConfig {
            relevance_threshold,
            chunk_size,
            dimensions,
            max_chunks: usize::MAX,
        };
        Self {
            inner: atp_context::ContextCompressor::with_config(config),
            chunk_size,
        }
    }

    /// Compress context for a task. Returns compression metrics.
    fn compress(&self, context: &[u8], task_type: &PyTaskType, task_prompt: &str) -> PyResult<PyCompressionResult> {
        let diff = self.inner
            .compress_for_task(context, task_type.inner, task_prompt.as_bytes())
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("{e}")))?;
        let orig = diff.original_size;
        let comp = diff.compressed_size;
        let total_chunks = if self.chunk_size > 0 {
            (context.len() + self.chunk_size - 1) / self.chunk_size
        } else {
            0
        };
        Ok(PyCompressionResult {
            original_size: orig,
            compressed_size: comp,
            compression_ratio: if comp > 0 { orig as f64 / comp as f64 } else { f64::INFINITY },
            chunks_kept: diff.chunks.len(),
            total_chunks,
            confidence: diff.confidence,
        })
    }

    fn __repr__(&self) -> String {
        "ContextCompressor()".to_string()
    }
}
