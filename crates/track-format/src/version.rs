//! Shared versions of the deterministic contracts and the independently versioned binary envelope.
/// Experimental handling contract; older replay hashes are incompatible.
pub const PHYSICS_VERSION: &str = "0.5.0";
/// Unchanged compiler contract.
pub const COMPILER_VERSION: &str = "0.2.0";
/// Fixed simulation rate (NFR2).
pub const TICK_HZ: u32 = 120;
/// Canonical input rate (D7): one `InputFrame` covers `TICKS_PER_FRAME` simulation ticks.
pub const INPUT_HZ: u32 = 60;
/// Simulation ticks covered by one input frame (`TICK_HZ / INPUT_HZ`).
pub const TICKS_PER_FRAME: u64 = 2;
/// Control state hash interval of `rumbo-replay/2`: every 10 s at 120 Hz (D17).
pub const CHECK_EVERY_TICKS: u64 = 1200;
/// Current replay format (D17). The writer always produces it.
pub const REPLAY_FORMAT_V2: &str = "rumbo-replay/2";
/// Legacy JSON replay format. Migration only: nothing outside the v1 → v2 tool reads it.
pub const REPLAY_FORMAT_V1: &str = "race-lab-replay/1";
/// Binary envelope version, little-endian u16 after magic.
pub const CODEC_VERSION: u16 = 1;
