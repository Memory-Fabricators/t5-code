//! Port of `src/services/oauth/crypto.ts` — PKCE helpers.

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use sha2::{Digest, Sha256};

fn base64_url_encode(bytes: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(bytes)
}

pub fn generate_code_verifier() -> String {
    let mut buf = [0u8; 32];
    rand::fill(&mut buf[..]);
    base64_url_encode(&buf)
}

pub fn generate_code_challenge(verifier: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(verifier.as_bytes());
    base64_url_encode(&hasher.finalize())
}

pub fn generate_state() -> String {
    let mut buf = [0u8; 32];
    rand::fill(&mut buf[..]);
    base64_url_encode(&buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn challenge_is_s256_of_verifier() {
        let verifier = "abc";
        let challenge = generate_code_challenge(verifier);
        let mut hasher = Sha256::new();
        hasher.update(b"abc");
        assert_eq!(challenge, base64_url_encode(&hasher.finalize()));
    }
}
