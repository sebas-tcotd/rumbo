//! Capa fina que expone `sim-core` a JavaScript. No contiene lógica de simulación:
//! si algo cambia el resultado de un replay, va en `sim-core`.

use sim_core::autopilot::Autopilot;
use sim_core::profile::{self, ProfileFile};
use sim_core::simulation::STATE_FIELDS;
use sim_core::{InputFrame, ReplayV2, Simulation, TrackDef, STATE_LEN};
use wasm_bindgen::prelude::*;

mod error;
pub use error::RumboError;
#[cfg(feature = "harness")]
pub mod harness;

fn parse_profile(json: &str) -> Result<ProfileFile, RumboError> {
    serde_json::from_str(json).map_err(|e| RumboError::InvalidProfile(e.to_string()))
}

fn decode_replay(bytes: &[u8]) -> Result<ReplayV2, RumboError> {
    ReplayV2::decode(bytes).map_err(|e| RumboError::InvalidReplay(e.to_string()))
}

/// Una sesión del laboratorio: pista + perfil congelado + auto.
#[wasm_bindgen]
pub struct Lab {
    sim: Simulation,
    autopilot: Autopilot,
    state: Vec<f32>,
}

#[wasm_bindgen]
impl Lab {
    /// Build a laboratory session.
    ///
    /// # Errors
    /// Returns a coded string when the track or profile is invalid.
    #[wasm_bindgen(constructor)]
    pub fn new(track_json: &str, profile_json: &str, seed: u32) -> Result<Lab, JsValue> {
        let track = TrackDef::from_json(track_json).map_err(RumboError::InvalidTrack)?;
        let profile = parse_profile(profile_json)?;
        let sim = Simulation::new(&track, &profile, seed as u64).map_err(RumboError::InvalidTrack)?;
        Ok(Lab { sim, autopilot: Autopilot::default(), state: vec![0.0; STATE_LEN] })
    }

    /// Dev tool (AR28): a session re-simulated from `rumbo-replay/2` bytes up to `tick`
    /// (`sim_core::replay::resimulate_until`). The replay does not carry the profile (D17): the host
    /// passes the one to use (the lab passes its current profile).
    ///
    /// # Errors
    /// Returns a coded string for an invalid track, profile, replay, or seek tick.
    pub fn from_replay(track_json: &str, replay: &[u8], profile_json: &str, tick: f64) -> Result<Lab, JsValue> {
        if !(tick.is_finite() && tick >= 0.0 && tick.fract() == 0.0) {
            return Err(RumboError::InvalidReplay(format!("tick inválido: {tick}")).into());
        }
        let track = TrackDef::from_json(track_json).map_err(RumboError::InvalidTrack)?;
        let profile = parse_profile(profile_json)?;
        let replay = decode_replay(replay)?;
        let sim = sim_core::replay::resimulate_until(&replay, &track, &profile, tick as u64)
            .map_err(RumboError::InvalidReplay)?;
        Ok(Lab { sim, autopilot: Autopilot::default(), state: vec![0.0; STATE_LEN] })
    }

    /// Dev tool (AR28): moves the car to lap distance `s`. Not recorded in the replay.
    pub fn teleport(&mut self, s: f32) {
        self.sim.teleport_to_s(s);
    }

    /// Avanza un tick con una entrada ya cuantizada. En el segundo tick de cada par la simulación ignora
    /// la entrada (D7): ver `accepts_frame`.
    pub fn step(&mut self, steer: i8, throttle: u8, brake: u8, flags: u8) {
        self.sim.step(InputFrame { steer, throttle, brake, flags });
    }

    /// Avanza un tick con ejes analógicos (dirección en [-1, 1], acelerador y freno en [0, 1]) usando la
    /// cuantización canónica con deadband (`InputFrame::from_axes`, D7).
    pub fn step_axes(&mut self, steer: f32, throttle: f32, brake: f32, flags: u8) {
        self.sim.step(InputFrame::from_axes(steer, throttle, brake, flags));
    }

    /// Si el próximo `step` toma una entrada nueva (primer tick del par, D7). El host solo consume
    /// eventos de un tick (reset, reinicio) cuando es verdadero; si no, se perderían.
    pub fn accepts_frame(&self) -> bool {
        self.sim.accepts_frame()
    }

    /// Perfil de la sesión (JSON de `ProfileFile`), para verificar su replay: el replay no lo lleva (D17).
    ///
    /// # Errors
    /// Devuelve el mensaje de serialización si el perfil no puede codificarse.
    pub fn profile_json(&self) -> Result<String, JsValue> {
        serde_json::to_string(self.sim.profile_file()).map_err(|e| RumboError::ExportFailed(e.to_string()).into())
    }

    /// Entrada que daría el piloto automático en el estado actual, empaquetada en un u32.
    /// El worker la aplica con `step`, así queda grabada como cualquier otra entrada.
    pub fn autopilot_input(&self) -> u32 {
        self.autopilot.input(&self.sim).pack()
    }

    /// Estado actual en el orden de `state_layout()`.
    pub fn state(&mut self) -> Vec<f32> {
        self.sim.write_state(&mut self.state);
        self.state.clone()
    }

    /// Refresh the fixed scratch buffer. JS copies before calling WASM again.
    pub fn state_ptr(&mut self) -> *const f32 {
        self.sim.write_state(&mut self.state);
        self.state.as_ptr()
    }

    pub fn tick(&self) -> f64 {
        self.sim.tick() as f64
    }

    pub fn state_hash(&self) -> String {
        self.sim.state_hash_hex()
    }

    pub fn track_hash(&self) -> String {
        self.sim.track_hash_hex()
    }

    pub fn profile_hash(&self) -> String {
        self.sim.profile_hash_hex()
    }

    pub fn track_length(&self) -> f32 {
        self.sim.track().length
    }

    /// Datos de la pista para el render (JSON): id, nombre, si es cerrada, salida y llegada.
    pub fn track_info(&self) -> String {
        let t = self.sim.track();
        serde_json::json!({
            "id": t.id,
            "name": t.name,
            "closed": t.closed,
            "startS": t.start_s,
            "finishS": t.finish_s,
            "length": t.length,
            "spacing": t.spacing,
        })
        .to_string()
    }

    /// Barreras de carrera para el render (JSON): [{ line: [[x,y,z], ...], arrow: ±1 }].
    pub fn barriers_json(&self) -> String {
        let b: Vec<_> = self
            .sim
            .track()
            .barriers
            .iter()
            .map(|b| serde_json::json!({ "line": b.line.iter().map(|p| [p.x, p.y, p.z]).collect::<Vec<_>>(), "arrow": b.arrow }))
            .collect();
        serde_json::Value::Array(b).to_string()
    }

    pub fn checkpoints(&self) -> Vec<f32> {
        self.sim.track().checkpoints.clone()
    }

    /// Partes de malla para el render: "drivable", "walls" o "decals".
    pub fn mesh_positions(&self, part: &str) -> Vec<f32> {
        self.part(part).map(|p| p.positions.clone()).unwrap_or_default()
    }

    pub fn mesh_surfaces(&self, part: &str) -> Vec<u8> {
        self.part(part).map(|p| p.surfaces.clone()).unwrap_or_default()
    }

    pub fn mesh_indices(&self, part: &str) -> Vec<u32> {
        self.part(part).map(|p| p.indices.clone()).unwrap_or_default()
    }

    /// Centro, derecha y normal de cada muestra del eje: [px,py,pz, rx,ry,rz, ux,uy,uz, ancho] × n.
    pub fn centerline(&self) -> Vec<f32> {
        let mut out = Vec::with_capacity(self.sim.track().samples.len() * 10);
        for s in &self.sim.track().samples {
            out.extend_from_slice(&[
                s.pos.x, s.pos.y, s.pos.z, s.right.x, s.right.y, s.right.z, s.up.x, s.up.y, s.up.z, s.width,
            ]);
        }
        out
    }

    pub fn ground_y(&self) -> f32 {
        self.sim.track().ground_y
    }

    /// Replay de la sesión completa en bytes `rumbo-replay/2`.
    ///
    /// # Errors
    ///
    /// Devuelve el mensaje del códec si el replay no puede codificarse.
    pub fn replay_bytes(&self) -> Result<Vec<u8>, JsValue> {
        self.sim.replay().encode().map_err(|e| RumboError::ExportFailed(e.to_string()).into())
    }

    fn part(&self, name: &str) -> Option<&sim_core::track::MeshPart> {
        let t = self.sim.track();
        match name {
            "drivable" => Some(&t.drivable),
            "walls" => Some(&t.walls),
            "decals" => Some(&t.decals),
            _ => None,
        }
    }
}

/// Nombres de los campos del estado, en orden (JSON: lista de strings).
#[wasm_bindgen]
pub fn state_layout() -> String {
    serde_json::to_string(STATE_FIELDS).unwrap_or_default()
}

/// Esquema de parámetros del perfil (JSON).
///
/// # Errors
/// Returns a coded string if schema serialization fails.
#[wasm_bindgen]
pub fn profile_schema() -> Result<String, JsValue> {
    profile::schema_json().map_err(|e| RumboError::SchemaFailed(e.to_string()).into())
}

/// Perfil por defecto (JSON de `ProfileFile`).
#[wasm_bindgen]
pub fn default_profile() -> String {
    serde_json::to_string(&ProfileFile {
        id: "hatch-arcade".into(),
        name: "Hatch arcade".into(),
        ..ProfileFile::default()
    })
    .unwrap_or_default()
}

/// Re-simula un replay `rumbo-replay/2` con el perfil dado y devuelve el informe (JSON de `VerifyReport`).
///
/// # Errors
/// Returns a coded string when the inputs are invalid or report serialization fails.
#[wasm_bindgen]
pub fn verify_replay(track_json: &str, replay: &[u8], profile_json: &str) -> Result<String, JsValue> {
    let track = TrackDef::from_json(track_json).map_err(RumboError::InvalidTrack)?;
    let profile = parse_profile(profile_json)?;
    let replay = decode_replay(replay)?;
    let report = sim_core::verify_replay(&replay, &track, &profile).map_err(RumboError::InvalidReplay)?;
    serde_json::to_string(&report).map_err(|e| RumboError::ExportFailed(e.to_string()).into())
}

/// Metadatos de un replay `rumbo-replay/2` (JSON), para que JavaScript los lea sin un segundo códec
/// postcard (D15, D16): `{ header, ticks, frames, checks: [{ tick, hash }], finalHash }`, con la cabecera
/// en la forma de `isReplayHeader` y los hashes en hexadecimal.
///
/// # Errors
/// Returns a coded string when the bytes are not a valid replay.
#[wasm_bindgen]
pub fn replay_summary(replay: &[u8]) -> Result<String, JsValue> {
    Ok(summary(&decode_replay(replay)?).to_string())
}

/// Summary JSON of a decoded replay (see `replay_summary`).
pub fn summary(replay: &ReplayV2) -> serde_json::Value {
    let checks: Vec<_> = replay
        .checks
        .iter()
        .map(|c| serde_json::json!({ "tick": c.tick, "hash": track_format::hash::to_hex(c.hash) }))
        .collect();
    serde_json::json!({
        "header": replay.header.wire_json(),
        "ticks": replay.ticks,
        "frames": ReplayV2::frames_for(replay.ticks),
        "checks": checks,
        "finalHash": track_format::hash::to_hex(replay.final_hash),
    })
}

/// Geometría del auto para el render: semiejes del chasis, radio de rueda y anclajes.
#[wasm_bindgen]
pub fn vehicle_geometry() -> String {
    use sim_core::vehicle::{HALF_EXTENTS, HARDPOINTS, WHEEL_RADIUS};
    let hp: Vec<[f32; 3]> = HARDPOINTS.iter().map(|h| [h.x, h.y, h.z]).collect();
    serde_json::json!({
        "halfExtents": [HALF_EXTENTS.x, HALF_EXTENTS.y, HALF_EXTENTS.z],
        "wheelRadius": WHEEL_RADIUS,
        "hardpoints": hp,
    })
    .to_string()
}

#[wasm_bindgen]
pub fn tick_hz() -> u32 {
    sim_core::TICK_HZ
}

#[wasm_bindgen]
pub fn physics_version() -> String {
    sim_core::PHYSICS_VERSION.to_string()
}

/// Hash bytes using the canonical track-format content identity.
#[wasm_bindgen]
pub fn hash_content(bytes: &[u8]) -> String {
    track_format::hash::content_hash(bytes).hex()
}

/// Versioned ABI descriptor derived from the simulation's actual field order.
#[wasm_bindgen]
pub fn snapshot_descriptor() -> String {
    let fields: Vec<_> = STATE_FIELDS
        .iter()
        .enumerate()
        .map(|(index, name)| serde_json::json!({"name": name, "type": "f32", "offset": index * size_of::<f32>()}))
        .collect();
    serde_json::json!({"version": 1, "stride": STATE_LEN * size_of::<f32>(), "fields": fields}).to_string()
}
