use pyo3::prelude::*;
use std::time::Duration;

/// Unique agent identifier.
#[pyclass]
#[derive(Clone)]
pub struct PyAgentId {
    pub inner: atp_types::AgentId,
}

#[pymethods]
impl PyAgentId {
    #[new]
    fn new() -> Self {
        Self {
            inner: atp_types::AgentId::new(),
        }
    }

    fn __str__(&self) -> String {
        self.inner.to_string()
    }

    fn __repr__(&self) -> String {
        format!("AgentId({})", self.inner)
    }
}

/// Task type enumeration.
#[pyclass]
#[derive(Clone)]
pub struct PyTaskType {
    pub inner: atp_types::TaskType,
}

#[pymethods]
impl PyTaskType {
    #[staticmethod]
    fn code_generation() -> Self {
        Self { inner: atp_types::TaskType::CodeGeneration }
    }

    #[staticmethod]
    fn analysis() -> Self {
        Self { inner: atp_types::TaskType::Analysis }
    }

    #[staticmethod]
    fn creative_writing() -> Self {
        Self { inner: atp_types::TaskType::CreativeWriting }
    }

    #[staticmethod]
    fn data_processing() -> Self {
        Self { inner: atp_types::TaskType::DataProcessing }
    }

    fn complexity_weight(&self) -> f64 {
        self.inner.complexity_weight()
    }

    fn __str__(&self) -> String {
        self.inner.to_string()
    }

    fn __repr__(&self) -> String {
        format!("TaskType.{}", self.inner)
    }
}

/// Quality-of-Service constraints.
#[pyclass]
#[derive(Clone)]
pub struct PyQoSConstraints {
    pub inner: atp_types::QoSConstraints,
}

#[pymethods]
impl PyQoSConstraints {
    #[new]
    #[pyo3(signature = (min_quality=0.7, max_latency_ms=10000, max_cost=1.0, min_trust=0.5))]
    fn new(min_quality: f64, max_latency_ms: u64, max_cost: f64, min_trust: f64) -> Self {
        Self {
            inner: atp_types::QoSConstraints {
                min_quality,
                max_latency: Duration::from_millis(max_latency_ms),
                max_cost,
                min_trust,
            },
        }
    }

    #[getter]
    fn min_quality(&self) -> f64 { self.inner.min_quality }

    #[getter]
    fn max_cost(&self) -> f64 { self.inner.max_cost }

    #[getter]
    fn min_trust(&self) -> f64 { self.inner.min_trust }

    fn relax(&self, factor: f64) -> Self {
        Self { inner: self.inner.relax(factor) }
    }

    fn __repr__(&self) -> String {
        format!(
            "QoSConstraints(min_quality={:.2}, max_cost={:.2}, min_trust={:.2})",
            self.inner.min_quality, self.inner.max_cost, self.inner.min_trust
        )
    }
}

/// Agent capability.
#[pyclass]
#[derive(Clone)]
pub struct PyCapability {
    pub inner: atp_types::Capability,
}

#[pymethods]
impl PyCapability {
    #[new]
    fn new(task_type: &PyTaskType, quality: f64, latency_ms: u64, cost: f64) -> Self {
        Self {
            inner: atp_types::Capability {
                task_type: task_type.inner,
                estimated_quality: quality,
                estimated_latency: Duration::from_millis(latency_ms),
                cost_per_task: cost,
            },
        }
    }

    #[getter]
    fn quality(&self) -> f64 { self.inner.estimated_quality }

    #[getter]
    fn cost(&self) -> f64 { self.inner.cost_per_task }

    fn __repr__(&self) -> String {
        format!(
            "Capability({}, q={:.2}, cost=${:.3})",
            self.inner.task_type, self.inner.estimated_quality, self.inner.cost_per_task
        )
    }
}

/// Per-capability trust score.
#[pyclass]
#[derive(Clone)]
pub struct PyTrustScore {
    #[pyo3(get)]
    pub score: f64,
    #[pyo3(get)]
    pub sample_count: u32,
    pub task_type: atp_types::TaskType,
}

#[pymethods]
impl PyTrustScore {
    fn __repr__(&self) -> String {
        format!("TrustScore({:.3}, n={})", self.score, self.sample_count)
    }
}
