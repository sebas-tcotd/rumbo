//! Replays: grabación de entradas por tick (RLE), manifest y verificación.

use crate::input::InputFrame;
use crate::profile::ProfileFile;
use crate::simulation::Simulation;
use crate::track::TrackDef;
use serde::{Deserialize, Serialize};

/// Cada cuántos ticks se guarda un hash de control (5 s a 120 Hz).
pub const CHECK_EVERY: u64 = 600;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct StateCheck {
    pub tick: u64,
    pub hash: String,
}

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

/// Todo lo necesario para reproducir una sesión exactamente.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReplayManifest {
    pub format: String,
    pub physics_version: String,
    pub compiler_version: String,
    pub tick_hz: u32,
    pub track_id: String,
    pub track_hash: String,
    pub profile: ProfileFile,
    pub profile_hash: String,
    pub seed: u64,
    pub ticks: u64,
    pub inputs: Vec<[u32; 2]>,
    pub checks: Vec<StateCheck>,
    pub final_state_hash: String,
    pub laps_ticks: Vec<u64>,
    pub invalid: bool,
}

pub const REPLAY_FORMAT: &str = "race-lab-replay/1";

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

impl ReplayManifest {
    pub fn from_json(s: &str) -> Result<Self, String> {
        serde_json::from_str(s).map_err(|e| format!("replay inválido: {e}"))
    }

    /// Re-simula el replay desde cero y compara hashes de control y estado final.
    pub fn verify(&self, track: &TrackDef) -> Result<VerifyReport, String> {
        if self.format != REPLAY_FORMAT {
            return Err(format!("formato desconocido: {}", self.format));
        }
        let mut sim = Simulation::new(track, &self.profile, self.seed)?;
        let fail = |message: String, ticks: u64| VerifyReport {
            ok: false,
            ticks,
            expected_final: self.final_state_hash.clone(),
            actual_final: String::new(),
            first_mismatch_tick: Some(0),
            laps_match: false,
            message,
        };
        if sim.track_hash_hex() != self.track_hash {
            return Ok(fail("la pista compilada no coincide con la del replay".into(), 0));
        }
        if sim.profile_hash_hex() != self.profile_hash {
            return Ok(fail("el perfil no coincide con su hash".into(), 0));
        }
        if self.physics_version != crate::PHYSICS_VERSION {
            return Ok(fail(
                format!("versión de física distinta: replay {}, actual {}", self.physics_version, crate::PHYSICS_VERSION),
                0,
            ));
        }

        let mut expected = self.checks.iter().peekable();
        let mut first_mismatch = None;
        for run in &self.inputs {
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
        let laps_match = sim.timer().completed == self.laps_ticks;
        let ok = first_mismatch.is_none() && actual == self.final_state_hash && laps_match && sim.tick() == self.ticks;
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
            expected_final: self.final_state_hash.clone(),
            actual_final: actual,
            first_mismatch_tick: first_mismatch,
            laps_match,
            message,
        })
    }
}
