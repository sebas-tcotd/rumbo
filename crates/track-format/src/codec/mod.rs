//! RTF\0 + little-endian u16 version + postcard payload.
//! The caller selects the DTO type; this envelope does not replace legacy JSON.
use crate::version::CODEC_VERSION;
use serde::{de::DeserializeOwned, Serialize};
const MAGIC: &[u8; 4] = b"RTF\0";
/// Explicit framing and serialization errors.
#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    TruncatedHeader,
    InvalidMagic,
    UnsupportedVersion(u16),
    Payload(postcard::Error),
    TrailingBytes,
}
impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "binary codec error: {self:?}")
    }
}
impl std::error::Error for Error {}
/// Encode a DTO using the current binary version.
/// # Errors
/// Returns a payload error if the DTO cannot be serialized.
pub fn encode<T: Serialize>(value: &T) -> Result<Vec<u8>, Error> {
    let payload = postcard::to_allocvec(value).map_err(Error::Payload)?;
    let mut bytes = Vec::with_capacity(6 + payload.len());
    bytes.extend_from_slice(MAGIC);
    bytes.extend_from_slice(&CODEC_VERSION.to_le_bytes());
    bytes.extend_from_slice(&payload);
    Ok(bytes)
}
/// Decode only a recognized envelope, requiring complete payload consumption.
/// # Errors
/// Rejects missing headers, unknown versions, invalid payloads and trailing bytes.
pub fn decode<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, Error> {
    if bytes.len() < 6 {
        return Err(Error::TruncatedHeader);
    }
    if &bytes[..4] != MAGIC {
        return Err(Error::InvalidMagic);
    }
    let version = u16::from_le_bytes([bytes[4], bytes[5]]);
    if version != CODEC_VERSION {
        return Err(Error::UnsupportedVersion(version));
    }
    let (value, rest) = postcard::take_from_bytes(&bytes[6..]).map_err(Error::Payload)?;
    if !rest.is_empty() {
        return Err(Error::TrailingBytes);
    }
    Ok(value)
}
