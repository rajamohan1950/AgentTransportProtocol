use pyo3::prelude::*;
use crate::py_types::*;

/// Helper to block on an async future from sync PyO3 context.
fn block_on<F: std::future::Future>(f: F) -> F::Output {
    tokio::runtime::Runtime::new().unwrap().block_on(f)
}

/// Ed25519 keypair for agent identity.
#[pyclass]
pub struct PyKeyPair {
    inner: atp_identity::KeyPair,
}

#[pymethods]
impl PyKeyPair {
    /// Generate a new random keypair.
    #[new]
    fn new() -> PyResult<Self> {
        let inner = atp_identity::KeyPair::generate()
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("{e}")))?;
        Ok(Self { inner })
    }

    /// Get the public key as hex string.
    fn public_key_hex(&self) -> String {
        hex_encode(&self.inner.public_key_bytes())
    }

    /// Get the DID URI (did:key:z6Mk...).
    fn did_uri(&self) -> PyResult<String> {
        let did = atp_identity::DidGenerator::generate_did(&self.inner.public_key_bytes())
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("{e}")))?;
        Ok(did.to_uri())
    }

    /// Sign a message. Returns signature as hex string.
    fn sign(&self, message: &[u8]) -> String {
        let sig = self.inner.sign(message);
        hex_encode(&sig.to_bytes())
    }

    /// Verify a signature (hex) against this keypair's public key.
    fn verify(&self, message: &[u8], signature_hex: &str) -> PyResult<bool> {
        let sig_bytes = hex_decode(signature_hex)?;
        if sig_bytes.len() != 64 {
            return Err(pyo3::exceptions::PyValueError::new_err("Signature must be 64 bytes"));
        }
        let mut arr = [0u8; 64];
        arr.copy_from_slice(&sig_bytes);
        let sig = ed25519_dalek::Signature::from_bytes(&arr);
        Ok(self.inner.verify(message, &sig).is_ok())
    }

    fn __repr__(&self) -> String {
        match self.did_uri() {
            Ok(uri) => format!("KeyPair(did={uri})"),
            Err(_) => "KeyPair(<error>)".to_string(),
        }
    }
}

/// In-memory identity and trust store.
#[pyclass]
pub struct PyIdentityStore {
    inner: atp_identity::IdentityStore,
}

#[pymethods]
impl PyIdentityStore {
    #[new]
    fn new() -> Self {
        Self {
            inner: atp_identity::IdentityStore::new(),
        }
    }

    /// Register a new agent with a fresh keypair. Returns the agent ID.
    fn register_agent(&self) -> PyResult<PyAgentId> {
        let kp = atp_identity::KeyPair::generate()
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("{e}")))?;
        let identity = atp_identity::DidGenerator::create_identity(&kp)
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("{e}")))?;
        let id = block_on(self.inner.register(identity))
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("{e}")))?;
        Ok(PyAgentId { inner: id })
    }

    /// Get trust score for an agent on a specific task type.
    fn trust_score(&self, agent: &PyAgentId, task_type: &PyTaskType) -> PyTrustScore {
        let ts = block_on(self.inner.trust_score(agent.inner, task_type.inner, chrono::Utc::now()));
        PyTrustScore {
            score: ts.score,
            sample_count: ts.sample_count,
            task_type: ts.task_type,
        }
    }

    /// Number of registered agents.
    fn agent_count(&self) -> usize {
        block_on(self.inner.list_agents()).len()
    }

    fn __repr__(&self) -> String {
        format!("IdentityStore(agents={})", self.agent_count())
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn hex_decode(hex: &str) -> PyResult<Vec<u8>> {
    (0..hex.len())
        .step_by(2)
        .map(|i| {
            u8::from_str_radix(&hex[i..i + 2], 16)
                .map_err(|e| pyo3::exceptions::PyValueError::new_err(format!("{e}")))
        })
        .collect()
}
