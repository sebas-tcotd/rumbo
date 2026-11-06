//! BLAKE3-128: first 16 digest bytes, in digest order.
/// A content identity; never substitutes legacy FNV state hashes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ContentHash(pub [u8; 16]);
impl ContentHash {
    /// Lowercase hexadecimal in digest byte order (32 characters).
    pub fn hex(self) -> String {
        self.0.iter().map(|b| format!("{b:02x}")).collect()
    }
}
/// Hash exact bytes and retain the first 16 bytes of BLAKE3.
pub fn content_hash(bytes: &[u8]) -> ContentHash {
    let digest = blake3::hash(bytes);
    let mut value = [0; 16];
    value.copy_from_slice(&digest.as_bytes()[..16]);
    ContentHash(value)
}
