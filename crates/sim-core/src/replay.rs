//! Replays: grabación de entradas por tick (RLE), manifest y verificación.

use crate::input::InputFrame;
use crate::simulation::Simulation;
use crate::track::TrackDef;
use serde::Serialize;

/// Cada cuántos ticks se guarda un hash de control (5 s a 120 Hz).
pub const CHECK_EVERY: u64 = 600;

pub use track_format::domain::replay::{ReplayManifest, StateCheck, REPLAY_FORMAT};

#[derive(Clone, Debug, Default)]
pub struct Recorder {
    /// Pares (repeticiones, entrada empaquetada).
    pub runs: Vec<[u32; 2]>,
    pub checks: Vec<StateCheck>,
}

impl Recorder {
    pub fn push(&mut self, input: InputFrame) {
        let packed = input.pack();
        match self.runs.last_mut() {
            Some(run) if run[1] == packed && run[0] < u32::MAX => run[0] += 1,
            _ => self.runs.push([1, packed]),
        }
    }

    pub fn ticks(&self) -> u64 {
        self.runs.iter().map(|r| r[0] as u64).sum()
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
    pub laps_match: bool,
    pub message: String,
}

/// Re-simulate a legacy replay and compare checkpoints, final state and laps.
/// # Errors
/// Rejects unknown replay formats and invalid track source data.
pub fn verify_replay(manifest: &ReplayManifest, track: &TrackDef) -> Result<VerifyReport, String> {
    if manifest.format != REPLAY_FORMAT {
        return Err(format!("formato desconocido: {}", manifest.format));
    }
    let mut sim = Simulation::new(track, &manifest.profile, manifest.seed)?;
    let fail = |message: String, ticks: u64| VerifyReport {
        ok: false,
        ticks,
        expected_final: manifest.final_state_hash.clone(),
        actual_final: String::new(),
        first_mismatch_tick: Some(0),
        laps_match: false,
        message,
    };
    if sim.track_hash_hex() != manifest.track_hash {
        return Ok(fail(
            "la pista compilada no coincide con la del replay".into(),
            0,
        ));
    }
    if sim.profile_hash_hex() != manifest.profile_hash {
        return Ok(fail("el perfil no coincide con su hash".into(), 0));
    }
    if manifest.physics_version != crate::PHYSICS_VERSION {
        return Ok(fail(
            format!(
                "versión de física distinta: replay {}, actual {}",
                manifest.physics_version,
                crate::PHYSICS_VERSION
            ),
            0,
        ));
    }

    let mut expected = manifest.checks.iter().peekable();
    let mut first_mismatch = None;
    for run in &manifest.inputs {
        let input = InputFrame::unpack(run[1]);
        for _ in 0..run[0] {
            sim.step(input);
            if let Some(check) = expected.peek() {
                if check.tick == sim.tick() {
                    if first_mismatch.is_none() && check.hash != sim.state_hash_hex() {
                        first_mismatch = Some(check.tick);
                    }
                    expected.next();
                }
            }
        }
    }
    let actual = sim.state_hash_hex();
    let laps_match = sim.timer().completed == manifest.laps_ticks;
    let ok = first_mismatch.is_none()
        && actual == manifest.final_state_hash
        && laps_match
        && sim.tick() == manifest.ticks;
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
        expected_final: manifest.final_state_hash.clone(),
        actual_final: actual,
        first_mismatch_tick: first_mismatch,
        laps_match,
        message,
    })
}

/// Upper bound of a dev re-simulation (30 min at 120 Hz), so a crafted file cannot hang the worker.
pub const MAX_SEEK_TICKS: u64 = 30 * 60 * 120;

/// Dev tool (AR28): re-simulates `manifest` from tick 0 up to `tick` and returns the live session there,
/// so the lab can inspect the state at a verification's `first_mismatch_tick`. Steps through
/// `Simulation::step`, so the returned session records the same inputs as the prefix of the replay.
/// # Errors
/// Same rejections as `verify_replay` for format, track, profile and physics version, plus a `tick`
/// beyond the replay or beyond `MAX_SEEK_TICKS`.
pub fn resimulate_until(manifest: &ReplayManifest, track: &TrackDef, tick: u64) -> Result<Simulation, String> {
    if manifest.format != REPLAY_FORMAT {
        return Err(format!("formato desconocido: {}", manifest.format));
    }
    if tick > manifest.ticks || tick > MAX_SEEK_TICKS {
        return Err(format!("tick fuera del replay: {tick} (máximo {})", manifest.ticks.min(MAX_SEEK_TICKS)));
    }
    if manifest.physics_version != crate::PHYSICS_VERSION {
        return Err(format!("versión de física distinta: replay {}", manifest.physics_version));
    }
    let mut sim = Simulation::new(track, &manifest.profile, manifest.seed)?;
    if sim.track_hash_hex() != manifest.track_hash {
        return Err("la pista compilada no coincide con la del replay".into());
    }
    if sim.profile_hash_hex() != manifest.profile_hash {
        return Err("el perfil no coincide con su hash".into());
    }
    'runs: for run in &manifest.inputs {
        let input = InputFrame::unpack(run[1]);
        for _ in 0..run[0] {
            if sim.tick() >= tick {
                break 'runs;
            }
            sim.step(input);
        }
    }
    Ok(sim)
}
