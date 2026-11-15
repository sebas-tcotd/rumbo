//! S4 harness (story 0.9): per-tick state-hash trace of a replay.
//!
//! Compiled only with the `harness` feature. The CI test binary (`ci/determinism/wasm/`) and the native
//! reference generator (`examples/trace_reference.rs`) enable it; the publication binary (`pkg/`, served
//! by `apps/web/dist`) does not, and `ci/bundle/check-wasm-size.mjs` fails if `trace_replay` leaks into it.
//! Nothing here runs inside the game loop: it only drives the public `Simulation` API from outside.
use crate::RumboError;
use sim_core::replay::REPLAY_FORMAT_V1;
use sim_core::{InputFrame, ReplayManifest, Simulation, TrackDef, PHYSICS_VERSION};
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

/// Re-simulates `replay_json` on `track_json` and records `Simulation::state_hash()` after every `step`,
/// with the same semantics as `crates/sim-core/tests/per_tick.rs` (inputs unpacked from the recorded runs).
/// Checks, final hash and laps keep being verified by `sim_core::verify_replay`.
///
/// # Errors
/// Invalid track or replay, unknown replay format or a different physics version.
pub fn trace(track_json: &str, replay_json: &str) -> Result<Trace, RumboError> {
    let track = TrackDef::from_json(track_json).map_err(RumboError::InvalidTrack)?;
    let manifest = ReplayManifest::from_json(replay_json).map_err(RumboError::InvalidReplay)?;
    if manifest.format != REPLAY_FORMAT_V1 {
        return Err(RumboError::InvalidReplay(format!("formato desconocido: {}", manifest.format)));
    }
    if manifest.physics_version != PHYSICS_VERSION {
        return Err(RumboError::InvalidReplay(format!(
            "versión de física distinta: replay {}, actual {PHYSICS_VERSION}",
            manifest.physics_version
        )));
    }
    let mut sim = Simulation::new(&track, &manifest.profile, manifest.seed).map_err(RumboError::InvalidTrack)?;
    let recorded: u64 = manifest.inputs.iter().map(|run| u64::from(run[0])).sum();
    let mut hex = String::with_capacity(usize::try_from(recorded).unwrap_or(0).saturating_mul(HEX_PER_TICK));
    for run in &manifest.inputs {
        let input = InputFrame::unpack(run[1]);
        for _ in 0..run[0] {
            sim.step(input);
            // Writing to a String cannot fail.
            let _ = write!(hex, "{:016x}", sim.state_hash());
        }
    }
    Ok(Trace { ticks: sim.tick(), hex })
}

/// `trace` for JavaScript: JSON `{ "ticks": n, "trace": "<16 hex digits per tick>" }`.
///
/// # Errors
/// Returns a coded string (see `RumboError`) when the trace cannot be produced.
#[wasm_bindgen]
pub fn trace_replay(track_json: &str, replay_json: &str) -> Result<String, JsValue> {
    let t = trace(track_json, replay_json)?;
    Ok(serde_json::json!({ "ticks": t.ticks, "trace": t.hex }).to_string())
}
