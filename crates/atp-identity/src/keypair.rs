//! Ed25519 key generation and management using `ed25519-dalek` 2.x.

use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use rand::rngs::OsRng;

use atp_types::IdentityError;

/// An Ed25519 signing key pair.
///
/// Wraps [`SigningKey`] and provides convenience methods for signing,
/// verification, and public-key extraction.
#[derive(Debug)]
pub struct KeyPair {
    signing_key: SigningKey,
}

impl KeyPair {
    // ── Construction ──────────────────────────────────────────────────

    /// Generate a fresh key pair using the OS CSPRNG.
    pub fn generate() -> Result<Self, IdentityError> {
        let mut csprng = OsRng;
        let signing_key = SigningKey::generate(&mut csprng);
        Ok(Self { signing_key })
    }

    /// Restore a key pair from a 32-byte seed (secret key bytes).
    pub fn from_bytes(bytes: &[u8; 32]) -> Result<Self, IdentityError> {
        let signing_key = SigningKey::from_bytes(bytes);
        Ok(Self { signing_key })
    }

    // ── Key access ───────────────────────────────────────────────────

    /// Return the raw 32-byte secret key.
    pub fn secret_key_bytes(&self) -> [u8; 32] {
        self.signing_key.to_bytes()
    }

    /// Return the compressed 32-byte public key.
    pub fn public_key_bytes(&self) -> [u8; 32] {
        self.signing_key.verifying_key().to_bytes()
    }

    /// Return the [`VerifyingKey`] (public key) suitable for
    /// distribution and DID construction.
    pub fn verifying_key(&self) -> VerifyingKey {
        self.signing_key.verifying_key()
    }

    // ── Signing ──────────────────────────────────────────────────────

    /// Produce a detached Ed25519 signature over arbitrary `message` bytes.
    pub fn sign(&self, message: &[u8]) -> Signature {
        self.signing_key.sign(message)
    }

    // ── Verification ─────────────────────────────────────────────────

    /// Verify a signature against the public key embedded in this pair.
    pub fn verify(&self, message: &[u8], signature: &Signature) -> Result<(), IdentityError> {
        self.signing_key
            .verifying_key()
            .verify_strict(message, signature)
            .map_err(|_| IdentityError::SignatureVerification)
    }

    /// Verify a signature against an *arbitrary* [`VerifyingKey`].
    pub fn verify_with_key(
        verifying_key: &VerifyingKey,
        message: &[u8],
        signature: &Signature,
    ) -> Result<(), IdentityError> {
        verifying_key
            .verify_strict(message, signature)
            .map_err(|_| IdentityError::SignatureVerification)
    }
}

impl Clone for KeyPair {
    fn clone(&self) -> Self {
        Self {
            signing_key: SigningKey::from_bytes(&self.signing_key.to_bytes()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_and_roundtrip() {
        let kp = KeyPair::generate().unwrap();
        let restored = KeyPair::from_bytes(&kp.secret_key_bytes()).unwrap();
        assert_eq!(kp.public_key_bytes(), restored.public_key_bytes());
    }

    #[test]
    fn sign_and_verify() {
        let kp = KeyPair::generate().unwrap();
        let msg = b"hello ATP";
        let sig = kp.sign(msg);
        assert!(kp.verify(msg, &sig).is_ok());
    }

    #[test]
    fn verify_rejects_wrong_message() {
        let kp = KeyPair::generate().unwrap();
        let sig = kp.sign(b"correct");
        assert!(kp.verify(b"wrong", &sig).is_err());
    }

    #[test]
    fn verify_with_external_key() {
        let kp = KeyPair::generate().unwrap();
        let msg = b"external";
        let sig = kp.sign(msg);
        let vk = kp.verifying_key();
        assert!(KeyPair::verify_with_key(&vk, msg, &sig).is_ok());
    }
}
