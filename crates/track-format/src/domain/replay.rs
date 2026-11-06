//! Legacy JSON replay DTO, unchanged field order and input packing.
use super::profile::ProfileFile;
pub use crate::version::REPLAY_FORMAT;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct StateCheck {
    pub tick: u64,
    pub hash: String,
}

/// Todo lo necesario para reproducir una sesión exactamente.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
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

impl ReplayManifest {
    /// Parse a legacy JSON manifest.
    /// # Errors
    /// Returns the JSON parser diagnostic for invalid replay data.
    pub fn from_json(s: &str) -> Result<Self, String> {
        serde_json::from_str(s).map_err(|e| format!("replay inválido: {e}"))
    }
}
