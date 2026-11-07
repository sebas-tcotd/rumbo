//! Shared legacy versions and the independently versioned binary envelope.
/// Experimental handling contract; older replay hashes are incompatible.
pub const PHYSICS_VERSION: &str = "0.5.0";
/// Unchanged compiler contract.
pub const COMPILER_VERSION: &str = "0.2.0";
/// Inputs are recorded at every simulation tick.
pub const TICK_HZ: u32 = 120;
/// Legacy JSON replay format.
pub const REPLAY_FORMAT: &str = "race-lab-replay/1";
/// Binary envelope version, little-endian u16 after magic.
pub const CODEC_VERSION: u16 = 1;
