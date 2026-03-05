use pyo3::prelude::*;

mod py_types;
mod py_identity;
mod py_context;
mod py_routing;
mod py_sim;

use py_types::*;
use py_identity::*;
use py_context::*;
use py_routing::*;
use py_sim::*;

/// Agent Transport Protocol - Python SDK
///
/// A five-layer protocol stack for trust-aware, economically-optimal
/// multi-agent networking.
///
/// Example:
///     import atp
///     sim = atp.Simulation(agents=50, seed=42)
///     results = sim.run_benchmark(tasks=10000)
///     print(f"Cost: ${results.avg_cost_per_task:.4f}")
#[pymodule]
fn atp(m: &Bound<'_, PyModule>) -> PyResult<()> {
    // Core types
    m.add_class::<PyAgentId>()?;
    m.add_class::<PyTaskType>()?;
    m.add_class::<PyQoSConstraints>()?;
    m.add_class::<PyCapability>()?;
    m.add_class::<PyTrustScore>()?;

    // Identity
    m.add_class::<PyKeyPair>()?;
    m.add_class::<PyIdentityStore>()?;

    // Context
    m.add_class::<PyContextCompressor>()?;
    m.add_class::<PyCompressionResult>()?;

    // Routing
    m.add_class::<PyEconomicRouter>()?;
    m.add_class::<PyRoute>()?;

    // Simulation
    m.add_class::<PySimulation>()?;
    m.add_class::<PyBenchMetrics>()?;

    Ok(())
}
