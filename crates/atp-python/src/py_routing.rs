use pyo3::prelude::*;
use atp_routing::AgentGraph;
use crate::py_types::*;

/// A computed route through the agent network.
#[pyclass]
#[derive(Clone)]
pub struct PyRoute {
    #[pyo3(get)]
    pub pattern: String,
    #[pyo3(get)]
    pub quality: f64,
    #[pyo3(get)]
    pub latency_ms: u64,
    #[pyo3(get)]
    pub cost: f64,
    #[pyo3(get)]
    pub agent_count: usize,
}

#[pymethods]
impl PyRoute {
    fn __repr__(&self) -> String {
        format!(
            "Route({}, q={:.3}, lat={}ms, cost=${:.4}, agents={})",
            self.pattern, self.quality, self.latency_ms, self.cost, self.agent_count
        )
    }
}

/// Economic router for multi-objective task routing.
#[pyclass]
pub struct PyEconomicRouter {
    inner: atp_routing::EconomicRouter,
}

#[pymethods]
impl PyEconomicRouter {
    /// Create a new router.
    #[new]
    fn new() -> Self {
        Self {
            inner: atp_routing::EconomicRouter::new(AgentGraph::new()),
        }
    }

    /// Add an agent with capabilities to the routing graph.
    fn add_agent(&mut self, agent: &PyAgentId, capabilities: Vec<PyCapability>, trust: f64) {
        let caps: Vec<atp_types::Capability> = capabilities.iter().map(|c| c.inner.clone()).collect();
        self.inner.add_agent(agent.inner, caps, trust);
    }

    /// Fully connect all agents in the graph.
    fn fully_connect(&mut self, latency_ms: u64) {
        self.inner.fully_connect(std::time::Duration::from_millis(latency_ms));
    }

    /// Find the best route for a task.
    fn find_route(&self, task_type: &PyTaskType, qos: &PyQoSConstraints) -> PyResult<PyRoute> {
        match self.inner.find_route(task_type.inner, &qos.inner, None) {
            Ok(route) => Ok(PyRoute {
                pattern: route.pattern.to_string(),
                quality: route.metrics.quality,
                latency_ms: route.metrics.latency.as_millis() as u64,
                cost: route.metrics.cost,
                agent_count: route.agents.len(),
            }),
            Err(e) => Err(pyo3::exceptions::PyRuntimeError::new_err(format!("{e}"))),
        }
    }

    /// Find multiple routes (Pareto front).
    fn find_routes(&self, task_type: &PyTaskType, qos: &PyQoSConstraints) -> PyResult<Vec<PyRoute>> {
        match self.inner.find_routes(task_type.inner, &qos.inner) {
            Ok(routes) => Ok(routes.into_iter().map(|r| PyRoute {
                pattern: r.pattern.to_string(),
                quality: r.metrics.quality,
                latency_ms: r.metrics.latency.as_millis() as u64,
                cost: r.metrics.cost,
                agent_count: r.agents.len(),
            }).collect()),
            Err(e) => Err(pyo3::exceptions::PyRuntimeError::new_err(format!("{e}"))),
        }
    }

    /// Number of agents in the routing graph.
    fn agent_count(&self) -> usize {
        self.inner.graph().node_count()
    }

    fn __repr__(&self) -> String {
        format!("EconomicRouter(agents={})", self.agent_count())
    }
}
