use atp_identity::{DidGenerator, KeyPair};

/// A cryptographic agent identity. Generate, sign, verify, print — that's it.
///
/// ```rust
/// let agent = atp_sdk::agent();
/// let sig = agent.sign(b"hello");
/// assert!(agent.verify(b"hello", &sig));
/// println!("{agent}");  // Agent(did:key:z6Mk...abc)
/// ```
pub struct Agent {
    keypair: KeyPair,
    did_uri: String,
}

/// An opaque signature. Just pass it back to `verify()`.
pub struct Signature {
    inner: ed25519_dalek::Signature,
}

impl Agent {
    /// Generate a new agent with a fresh Ed25519 keypair and DID.
    pub fn new() -> Self {
        let keypair = KeyPair::generate().expect("key generation failed");
        let did = DidGenerator::generate_did(&keypair.public_key_bytes())
            .expect("DID generation failed");
        let did_uri = did.to_uri();
        Self { keypair, did_uri }
    }

    /// The agent's DID URI (e.g., `"did:key:z6Mk..."`).
    pub fn did(&self) -> &str {
        &self.did_uri
    }

    /// Sign arbitrary bytes.
    pub fn sign(&self, message: &[u8]) -> Signature {
        Signature {
            inner: self.keypair.sign(message),
        }
    }

    /// Verify a signature against this agent's public key.
    pub fn verify(&self, message: &[u8], sig: &Signature) -> bool {
        self.keypair.verify(message, &sig.inner).is_ok()
    }

    /// The raw 32-byte public key as a hex string.
    pub fn public_key_hex(&self) -> String {
        self.keypair
            .public_key_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    }
}

impl Default for Agent {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for Agent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let uri = &self.did_uri;
        if uri.len() > 30 {
            write!(f, "Agent({}...{})", &uri[..20], &uri[uri.len() - 6..])
        } else {
            write!(f, "Agent({uri})")
        }
    }
}

impl std::fmt::Debug for Agent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Agent({})", self.did_uri)
    }
}

impl std::fmt::Display for Signature {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let bytes = self.inner.to_bytes();
        let hex: String = bytes.iter().take(8).map(|b| format!("{b:02x}")).collect();
        write!(f, "Sig({hex}...)")
    }
}

impl std::fmt::Debug for Signature {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self}")
    }
}
