//! BLAKE3-128: first 16 digest bytes, in digest order.
use serde::{Deserialize, Serialize};

/// A content identity; never substitutes legacy FNV state hashes.
///
/// Postcard writes it as 16 raw bytes (a fixed-size array has no length prefix).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ContentHash(pub [u8; 16]);
impl ContentHash {
    /// Lowercase hexadecimal in digest byte order (32 characters).
    pub fn hex(self) -> String {
        self.0.iter().map(|b| format!("{b:02x}")).collect()
    }

    /// Parse exactly 32 lowercase hexadecimal characters (the inverse of `hex`).
    pub fn from_hex(s: &str) -> Option<Self> {
        let bytes = s.as_bytes();
        if bytes.len() != 32 {
            return None;
        }
        let mut value = [0; 16];
        for (i, [hi, lo]) in bytes.as_chunks::<2>().0.iter().enumerate() {
            value[i] = (hex_digit(*hi)? << 4) | hex_digit(*lo)?;
        }
        Some(Self(value))
    }
}

fn hex_digit(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        _ => None,
    }
}

/// Hash exact bytes and retain the first 16 bytes of BLAKE3.
pub fn content_hash(bytes: &[u8]) -> ContentHash {
    let digest = blake3::hash(bytes);
    let mut value = [0; 16];
    value.copy_from_slice(&digest.as_bytes()[..16]);
    ContentHash(value)
}
