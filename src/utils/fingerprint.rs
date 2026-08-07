//! Port of `src/utils/fingerprint.ts`.

use sha2::{Digest, Sha256};

/// Hardcoded salt from backend validation — must match TS exactly.
pub const FINGERPRINT_SALT: &str = "59cf53e54c78";

/// Computes 3-character fingerprint for Claude Code attribution.
/// Algorithm: SHA256(SALT + msg[4] + msg[7] + msg[20] + version)[:3]
pub fn compute_fingerprint(message_text: &str, version: &str) -> String {
    let indices = [4usize, 7, 20];
    let chars: String = indices
        .iter()
        .map(|&i| message_text.chars().nth(i).unwrap_or('0'))
        .collect();
    let input = format!("{FINGERPRINT_SALT}{chars}{version}");
    let hash = Sha256::digest(input.as_bytes());
    hex_first3(&hash)
}

fn hex_first3(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(3);
    for b in bytes {
        let hi = b >> 4;
        let lo = b & 0xf;
        // only need first 3 hex chars = 1.5 bytes → take first 2 bytes partially
        for nibble in [hi, lo] {
            if s.len() >= 3 {
                break;
            }
            s.push(char::from_digit(nibble as u32, 16).unwrap_or('0'));
        }
        if s.len() >= 3 {
            break;
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprint_known_shape() {
        let fp = compute_fingerprint("Reply with exactly the single word: pong", "0.1.0");
        assert_eq!(fp.len(), 3);
        assert_eq!(fp, "e6a"); // matched probe script
    }
}
