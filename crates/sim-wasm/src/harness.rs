//! S4 harness (story 0.9, `rumbo-replay/2` since 0.10): per-tick state-hash trace of a replay.
//!
//! Compiled only with the `harness` feature. The CI test binary (`ci/determinism/wasm/`) and the native
//! reference generator (`examples/trace_reference.rs`) enable it; the publication binary (`pkg/`, served
//! by `apps/web/dist`) does not, and `ci/bundle/check-wasm-size.mjs` fails if `trace_replay` leaks into it.
//! Nothing here runs inside the game loop: it only drives the public `Simulation` API from outside.
use crate::RumboError;
use sim_core::{ProfileFile, ReplayV2, Simulation, TrackDef, PHYSICS_VERSION};
use std::fmt::Write as _;
use wasm_bindgen::prelude::*;

/// Hex digits per tick in a trace: one `u64` state hash, lossless.
pub const HEX_PER_TICK: usize = 16;

/// Per-tick trace of one replay.
#[derive(Debug)]
pub struct Trace {
    /// Ticks simulated (one hash each).
    pub ticks: u64,
    /// `Simulation::state_hash()` after every `step`, each as 16 lowercase hex digits, in tick order.
    pub hex: String,
}

/// Re-simulates the golden `replay` (`rumbo-replay/2` bytes) on `track_json` and records
/// `Simulation::state_hash()` after every `step`, with the same semantics as
/// `crates/sim-core/tests/per_tick.rs`: each tick receives its pair's frame and the simulation applies
/// the 60 Hz rule (D7). The goldens carry `LEGACY_VEHICLE` and were recorded with
/// `ProfileFile::default()`, which is the profile used here. Checks and the final hash keep being
/// verified by `sim_core::verify_replay`.
///
/// # Errors
/// Invalid track or replay bytes, or a different physics version.
pub fn trace(track_json: &str, replay: &[u8]) -> Result<Trace, RumboError> {
    let track = TrackDef::from_json(track_json).map_err(RumboError::InvalidTrack)?;
    let replay = ReplayV2::decode(replay).map_err(|e| RumboError::InvalidReplay(e.to_string()))?;
    if replay.header.physics_version != PHYSICS_VERSION {
        return Err(RumboError::InvalidReplay(format!(
            "versión de física distinta: replay {}, actual {PHYSICS_VERSION}",
            replay.header.physics_version
        )));
    }
    let mut sim =
        Simulation::new(&track, &ProfileFile::default(), replay.header.seed).map_err(RumboError::InvalidTrack)?;
    let capacity = usize::try_from(replay.ticks).unwrap_or(0).saturating_mul(HEX_PER_TICK);
    let mut hex = String::with_capacity(capacity);
    for frame in replay.tick_frames() {
        sim.step(frame);
        // Writing to a String cannot fail.
        let _ = write!(hex, "{:016x}", sim.state_hash());
    }
    Ok(Trace { ticks: sim.tick(), hex })
}

/// `trace` for JavaScript: JSON `{ "ticks": n, "trace": "<16 hex digits per tick>" }`.
///
/// # Errors
/// Returns a coded string (see `RumboError`) when the trace cannot be produced.
#[wasm_bindgen]
pub fn trace_replay(track_json: &str, replay: &[u8]) -> Result<String, JsValue> {
    let t = trace(track_json, replay)?;
    Ok(serde_json::json!({ "ticks": t.ticks, "trace": t.hex }).to_string())
}
