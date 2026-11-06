//! Legacy FNV and new content identities serve distinct protocols.
mod content;
mod fnv;
pub use content::{content_hash, ContentHash};
pub use fnv::{to_hex, Fnv64};
