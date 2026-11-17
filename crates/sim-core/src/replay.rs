//! Replays: 60 Hz recording (`rumbo-replay/2`), verification and dev re-simulation.
//!
//! The recorder keeps one `InputFrame` per pair of ticks (D7) as canonical runs, plus a control state
//! hash every `CHECK_EVERY_TICKS`. Verification re-simulates the frames tick by tick with
//! `Simulation::step`, which applies the pairing rule itself.

use crate::input::InputFrame;
use crate::profile::ProfileFile;
use crate::simulation::Simulation;
use crate::track::TrackDef;
use serde::Serialize;
use track_format::domain::replay_v2::{ControlHash, InputRun, ReplayV2};

pub use track_format::domain::replay::{ReplayManifest, StateCheck, REPLAY_FORMAT_V1};
pub use track_format::domain::replay_v2::{legacy_race, legacy_track_hash, VehicleRef};
pub use track_format::version::{CHECK_EVERY_TICKS, REPLAY_FORMAT_V2};

/// Recorded session: one frame per pair of ticks, as canonical runs, and the control hashes.
#[derive(Clone, Debug, Default)]
pub struct Recorder {
    pub runs: Vec<InputRun>,
    pub checks: Vec<ControlHash>,
}

impl Recorder {
    /// Append the frame of a new pair of ticks.
    pub fn push(&mut self, frame: InputFrame) {
        match self.runs.last_mut() {
            Some(run) if run.frame == frame => run.frames += 1,
            _ => self.runs.push(InputRun { frames: 1, frame }),
        }
    }

    /// Recorded 60 Hz frames.
    pub fn frames(&self) -> u64 {
        self.runs.iter().map(|r| r.frames).sum()
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifyReport {
    pub ok: bool,
    pub ticks: u64,
    pub expected_final: String,
    pub actual_final: String,
    pub first_mismatch_tick: Option<u64>,
    /// Every control hash was reached in order and matched. Laps need no separate check: the lap timer
    /// is part of the state hash.
    pub checks_match: bool,
    /// False while the replay carries `LEGACY_VEHICLE`: the caller supplied the profile and nothing
    /// could check it against the replay (story 1.3 introduces hashed vehicle profiles).
    pub profile_verified: bool,
    pub message: String,
}

/// Header checks shared by verification and dev re-simulation: physics version, track identity,
/// and the E0 sentinels for race and vehicle.
/// # Errors
/// A message in Spanish naming the first mismatch.
fn check_header(replay: &ReplayV2, sim: &Simulation) -> Result<(), String> {
    let h = &replay.header;
    if h.physics_version != crate::PHYSICS_VERSION {
        return Err(format!(
            "versión de física distinta: replay {}, actual {}",
            h.physics_version,
            crate::PHYSICS_VERSION
        ));
    }
    if h.track_hash != sim.track_content_hash() {
        return Err("la pista compilada no coincide con la del replay".into());
    }
    if h.race_hash != legacy_race() {
        return Err("la carrera del replay todavía no se puede verificar (llega con la 1.1 y la 2.5)".into());
    }
    if !h.vehicle_ref.is_legacy() {
        return Err("el perfil de vehículo del replay todavía no se puede verificar (llega con la 1.3)".into());
    }
    Ok(())
}

/// Re-simulate a replay and compare its control hashes, final state and declared ticks.
///
/// The replay does not carry the vehicle profile (D17): `profile` is supplied by the caller. Under the
/// `LEGACY_VEHICLE` sentinel it cannot be checked, and the report says so (`profile_verified: false`).
/// # Errors
/// Invalid track source data.
pub fn verify_replay(replay: &ReplayV2, track: &TrackDef, profile: &ProfileFile) -> Result<VerifyReport, String> {
    let mut sim = Simulation::new(track, profile, replay.header.seed)?;
    let expected_final = track_format::hash::to_hex(replay.final_hash);
    if let Err(message) = check_header(replay, &sim) {
        return Ok(VerifyReport {
            ok: false,
            ticks: 0,
            expected_final,
            actual_final: String::new(),
            first_mismatch_tick: Some(0),
            checks_match: false,
            profile_verified: false,
            message,
        });
    }
    let mut expected = replay.checks.iter().peekable();
    let mut first_mismatch = None;
    for frame in replay.tick_frames() {
        sim.step(frame);
        if let Some(check) = expected.next_if(|c| c.tick == sim.tick()) {
            if first_mismatch.is_none() && check.hash != sim.state_hash() {
                first_mismatch = Some(check.tick);
            }
        }
    }
    let actual = sim.state_hash();
    let checks_match = first_mismatch.is_none() && expected.next().is_none();
    let ok = checks_match && actual == replay.final_hash && sim.tick() == replay.ticks;
    let message = if ok {
        "replay reproducido de forma idéntica".to_string()
    } else if let Some(t) = first_mismatch {
        format!("divergencia detectada en el tick {t}")
    } else {
        "el estado final no coincide".to_string()
    };
    Ok(VerifyReport {
        ok,
        ticks: sim.tick(),
        expected_final,
        actual_final: track_format::hash::to_hex(actual),
        first_mismatch_tick: first_mismatch,
        checks_match,
        profile_verified: false,
        message,
    })
}

/// Migration only (story 0.10): the 60 Hz frames of a `race-lab-replay/1` session. Frame `k` takes the
/// axes and the held handbrake of tick `2k` and the union of the event flags (`RESET`, `RESTART`) of
/// ticks `2k` and `2k + 1`, so no event recorded in v1 is lost.
pub fn v1_frames(manifest: &ReplayManifest) -> Vec<InputFrame> {
    let ticks: Vec<InputFrame> = manifest
        .inputs
        .iter()
        .flat_map(|run| core::iter::repeat_n(InputFrame::unpack(run[1]), usize::try_from(run[0]).unwrap_or(0)))
        .collect();
    ticks
        .chunks(2)
        .map(|pair| {
            let events = pair.iter().fold(0, |f, i| f | (i.flags & crate::input::EVENT_FLAGS));
            InputFrame { flags: (pair[0].flags & !crate::input::EVENT_FLAGS) | events, ..pair[0] }
        })
        .collect()
}

/// Migration only (story 0.10): re-simulates the converted frames of a v1 session for its declared ticks
/// under the current physics and returns the resulting `rumbo-replay/2`. Deterministic: equal inputs give
/// equal replays (and bytes).
/// # Errors
/// Invalid track source data, or v1 inputs that do not cover the declared ticks.
pub fn migrate_v1(manifest: &ReplayManifest, track: &TrackDef) -> Result<ReplayV2, String> {
    let frames = v1_frames(manifest);
    if (frames.len() as u64) < ReplayV2::frames_for(manifest.ticks) {
        return Err(format!("el replay v1 no cubre sus {} ticks", manifest.ticks));
    }
    let mut sim = Simulation::new(track, &manifest.profile, manifest.seed)?;
    for t in 0..manifest.ticks {
        sim.step(frames[usize::try_from(t / 2).unwrap_or(usize::MAX)]);
    }
    Ok(sim.replay())
}

/// Upper bound of a dev re-simulation (30 min at 120 Hz), so a crafted file cannot hang the worker.
pub const MAX_SEEK_TICKS: u64 = 30 * 60 * 120;

/// Dev tool (AR28): re-simulates `replay` from tick 0 up to `tick` and returns the live session there,
/// so the lab can inspect the state at a verification's `first_mismatch_tick`. Steps through
/// `Simulation::step`, so the returned session records the same frames as the prefix of the replay.
/// # Errors
/// Same header rejections as `verify_replay`, plus a `tick` beyond the replay or beyond `MAX_SEEK_TICKS`.
pub fn resimulate_until(
    replay: &ReplayV2,
    track: &TrackDef,
    profile: &ProfileFile,
    tick: u64,
) -> Result<Simulation, String> {
    let max = replay.ticks.min(MAX_SEEK_TICKS);
    if tick > max {
        return Err(format!("tick fuera del replay: {tick} (máximo {max})"));
    }
    let mut sim = Simulation::new(track, profile, replay.header.seed)?;
    check_header(replay, &sim)?;
    for frame in replay.tick_frames().take(usize::try_from(tick).unwrap_or(usize::MAX)) {
        sim.step(frame);
    }
    debug_assert_eq!(sim.tick(), tick, "the replay covers `tick` (checked above)");
    Ok(sim)
}
