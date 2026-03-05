//! W3C DID generation using the `did:key` method (Ed25519).
//!
//! The encoding follows the did:key specification:
//!   1. Prepend the multicodec prefix for Ed25519 public key (0xed01).
//!   2. Base58-btc encode the result.
//!   3. Prefix with `z` (multibase indicator for base58btc).

use atp_types::{AgentId, AgentIdentity, Did, IdentityError};
use chrono::Utc;

use crate::keypair::KeyPair;

/// Multicodec prefix for Ed25519 public keys.
const ED25519_MULTICODEC_PREFIX: [u8; 2] = [0xed, 0x01];

/// Base58-btc alphabet (same as Bitcoin).
const BASE58_ALPHABET: &[u8; 58] =
    b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";

/// Generator for `did:key` identifiers backed by Ed25519 keys.
pub struct DidGenerator;

impl DidGenerator {
    /// Create a new `Did` from an Ed25519 public key (32 bytes).
    ///
    /// Returns a `Did` whose `method` is `"key"` and whose `identifier`
    /// is the multibase-base58btc-encoded multicodec value.
    pub fn generate_did(public_key: &[u8; 32]) -> Result<Did, IdentityError> {
        // Build multicodec payload: 0xed01 || public_key
        let mut mc_bytes = Vec::with_capacity(2 + 32);
        mc_bytes.extend_from_slice(&ED25519_MULTICODEC_PREFIX);
        mc_bytes.extend_from_slice(public_key);

        let encoded = base58_encode(&mc_bytes);

        // Multibase prefix 'z' denotes base58btc
        let identifier = format!("z{encoded}");

        Ok(Did {
            method: "key".to_string(),
            identifier,
        })
    }

    /// Convenience alias matching the pre-existing API.
    pub fn from_public_key(public_key_bytes: &[u8; 32]) -> Did {
        Self::generate_did(public_key_bytes)
            .expect("DID generation from valid key bytes should not fail")
    }

    /// Generate a full `AgentIdentity` from a `KeyPair`.
    ///
    /// The identity bundles the agent id, DID, raw public key bytes,
    /// an empty capability list, and the current UTC timestamp.
    pub fn create_identity(keypair: &KeyPair) -> Result<AgentIdentity, IdentityError> {
        let pub_bytes = keypair.public_key_bytes();
        let did = Self::generate_did(&pub_bytes)?;

        Ok(AgentIdentity {
            id: AgentId::new(),
            did,
            public_key: pub_bytes.to_vec(),
            capabilities: Vec::new(),
            created_at: Utc::now(),
        })
    }

    /// Verify that a `Did` is well-formed and consistent with the
    /// supplied public key.
    pub fn verify_did(did: &Did, public_key: &[u8; 32]) -> Result<bool, IdentityError> {
        if did.method != "key" {
            return Err(IdentityError::InvalidDid(format!(
                "unsupported DID method: {}",
                did.method
            )));
        }

        let expected = Self::generate_did(public_key)?;
        Ok(did.identifier == expected.identifier)
    }

    /// Extract the 32-byte Ed25519 public key from a `did:key` identifier.
    ///
    /// The identifier must start with `z` (multibase base58btc) and decode
    /// to a 34-byte payload (2-byte multicodec prefix + 32-byte key).
    pub fn extract_public_key(did: &Did) -> Result<[u8; 32], IdentityError> {
        if did.method != "key" {
            return Err(IdentityError::InvalidDid(format!(
                "unsupported DID method: {}",
                did.method
            )));
        }

        let ident = &did.identifier;
        if !ident.starts_with('z') {
            return Err(IdentityError::InvalidDid(
                "identifier must start with 'z' (base58btc multibase)".to_string(),
            ));
        }

        let decoded = base58_decode(&ident[1..]).map_err(|e| {
            IdentityError::InvalidDid(format!("base58 decode failed: {e}"))
        })?;

        if decoded.len() != 34 {
            return Err(IdentityError::InvalidDid(format!(
                "decoded length {} != 34",
                decoded.len()
            )));
        }

        if decoded[0] != ED25519_MULTICODEC_PREFIX[0]
            || decoded[1] != ED25519_MULTICODEC_PREFIX[1]
        {
            return Err(IdentityError::InvalidDid(
                "multicodec prefix is not Ed25519 (0xed01)".to_string(),
            ));
        }

        let mut key = [0u8; 32];
        key.copy_from_slice(&decoded[2..34]);
        Ok(key)
    }
}

// -- Base58 (Bitcoin alphabet) encoder/decoder --

/// Encode bytes to base58btc (Bitcoin alphabet). Faithfully preserves
/// leading zero bytes as leading '1' characters per the standard algorithm.
fn base58_encode(input: &[u8]) -> String {
    if input.is_empty() {
        return String::new();
    }

    // Count leading zeros
    let leading_zeros = input.iter().take_while(|&&b| b == 0).count();

    // Convert byte array to a big integer (big-endian) stored as a Vec<u32>
    // and repeatedly divide by 58.
    let mut num: Vec<u32> = vec![0];
    for &byte in input {
        let mut carry = byte as u32;
        for digit in num.iter_mut() {
            carry += *digit * 256;
            *digit = carry % 58;
            carry /= 58;
        }
        while carry > 0 {
            num.push(carry % 58);
            carry /= 58;
        }
    }

    let mut result = String::with_capacity(leading_zeros + num.len());

    // Leading zero bytes map to '1'
    for _ in 0..leading_zeros {
        result.push('1');
    }

    // Digits are in little-endian order in `num`; reverse for output.
    for &d in num.iter().rev() {
        result.push(BASE58_ALPHABET[d as usize] as char);
    }

    result
}

/// Decode a base58btc string back to bytes.
fn base58_decode(input: &str) -> Result<Vec<u8>, String> {
    if input.is_empty() {
        return Ok(Vec::new());
    }

    let leading_ones = input.chars().take_while(|&c| c == '1').count();

    // Build a big integer in base-256 (little-endian digits).
    let mut num: Vec<u32> = vec![0];
    for ch in input.chars() {
        let val = BASE58_ALPHABET
            .iter()
            .position(|&c| c == ch as u8)
            .ok_or_else(|| format!("invalid base58 character: {ch}"))? as u32;

        let mut carry = val;
        for digit in num.iter_mut() {
            carry += *digit * 58;
            *digit = carry % 256;
            carry /= 256;
        }
        while carry > 0 {
            num.push(carry % 256);
            carry /= 256;
        }
    }

    // Remove trailing zeros from the big-endian perspective (leading in LE vec).
    while num.len() > 1 && *num.last().unwrap() == 0 {
        num.pop();
    }

    let mut result = vec![0u8; leading_ones];
    result.reserve(num.len());
    for &d in num.iter().rev() {
        result.push(d as u8);
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keypair::KeyPair;

    #[test]
    fn did_roundtrip() {
        let kp = KeyPair::generate().unwrap();
        let pub_bytes = kp.public_key_bytes();
        let did = DidGenerator::generate_did(&pub_bytes).unwrap();

        assert_eq!(did.method, "key");
        assert!(did.identifier.starts_with('z'));

        // Verify round-trip
        assert!(DidGenerator::verify_did(&did, &pub_bytes).unwrap());
    }

    #[test]
    fn did_uri_format() {
        let kp = KeyPair::generate().unwrap();
        let did = DidGenerator::generate_did(&kp.public_key_bytes()).unwrap();
        let uri = did.to_uri();
        assert!(uri.starts_with("did:key:z"));
    }

    #[test]
    fn extract_public_key_roundtrip() {
        let kp = KeyPair::generate().unwrap();
        let pub_bytes = kp.public_key_bytes();
        let did = DidGenerator::generate_did(&pub_bytes).unwrap();
        let extracted = DidGenerator::extract_public_key(&did).unwrap();
        assert_eq!(pub_bytes, extracted);
    }

    #[test]
    fn create_identity_works() {
        let kp = KeyPair::generate().unwrap();
        let identity = DidGenerator::create_identity(&kp).unwrap();
        assert_eq!(identity.public_key, kp.public_key_bytes().to_vec());
        assert!(identity.did.to_uri().starts_with("did:key:z"));
    }

    #[test]
    fn from_public_key_compat() {
        let kp = KeyPair::generate().unwrap();
        let pub_bytes = kp.public_key_bytes();
        let did = DidGenerator::from_public_key(&pub_bytes);
        assert_eq!(did.method, "key");
        assert!(did.identifier.starts_with('z'));
    }

    #[test]
    fn base58_encode_decode_roundtrip() {
        let data = b"hello world";
        let encoded = base58_encode(data);
        let decoded = base58_decode(&encoded).unwrap();
        assert_eq!(decoded, data);
    }

    #[test]
    fn base58_leading_zeros() {
        let data = [0u8, 0, 0, 1, 2, 3];
        let encoded = base58_encode(&data);
        let decoded = base58_decode(&encoded).unwrap();
        assert_eq!(decoded, data);
    }

    #[test]
    fn wrong_method_rejected() {
        let did = Did {
            method: "web".to_string(),
            identifier: "example.com".to_string(),
        };
        let key = [0u8; 32];
        assert!(DidGenerator::verify_did(&did, &key).is_err());
    }
}
