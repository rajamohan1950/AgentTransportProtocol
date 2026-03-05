use pyo3::prelude::*;

mod py_types;
mod py_identity;
mod py_context;
mod py_routing;
mod py_sim;
mod py_sdk;

use py_types::*;
use py_identity::*;
use py_context::*;
use py_routing::*;
use py_sim::*;
use py_sdk::*;

/// Agent Transport Protocol - Python SDK
///
/// Dead simple:
///     import atp
///     atp.benchmark()           # full table
///     atp.route("coding")       # best route
///     atp.compress(b"...", "coding")  # 28x compression
///
/// Or granular:
///     sim = atp.Simulation(agents=50, seed=42)
///     results = sim.run_benchmark(tasks=10000)
///     print(f"Cost: ${results.avg_cost_per_task:.4f}")
#[pymodule]
fn atp(m: &Bound<'_, PyModule>) -> PyResult<()> {
    // === DEAD-SIMPLE FREE FUNCTIONS ===
    m.add_function(wrap_pyfunction!(py_benchmark, m)?)?;
    m.add_function(wrap_pyfunction!(py_route, m)?)?;
    m.add_function(wrap_pyfunction!(py_compress, m)?)?;
    m.add_function(wrap_pyfunction!(py_sign, m)?)?;
    m.add_function(wrap_pyfunction!(py_trust, m)?)?;

    // === GRANULAR API ===
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
