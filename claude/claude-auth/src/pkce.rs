//! PKCE (Proof Key for Code Exchange) utilities

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use rand::{rngs::OsRng, RngCore};
use sha2::{Digest, Sha256};

/// Generate a cryptographically random code verifier
pub fn generate_code_verifier() -> String {
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

/// Generate S256 code challenge from code verifier
pub fn code_challenge_s256(verifier: &str) -> String {
    let digest = Sha256::digest(verifier.as_bytes());
    URL_SAFE_NO_PAD.encode(digest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_verifier_length() {
        let verifier = generate_code_verifier();
        assert!(verifier.len() >= 43);
    }

    #[test]
    fn test_challenge_deterministic() {
        let verifier = "test_verifier_string";
        let c1 = code_challenge_s256(verifier);
        let c2 = code_challenge_s256(verifier);
        assert_eq!(c1, c2);
    }
}
