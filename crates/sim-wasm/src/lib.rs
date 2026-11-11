//! Capa fina que expone `sim-core` a JavaScript. No contiene lógica de simulación:
//! si algo cambia el resultado de un replay, va en `sim-core`.

use sim_core::autopilot::Autopilot;
use sim_core::profile::{self, ProfileFile};
use sim_core::simulation::STATE_FIELDS;
use sim_core::{InputFrame, ReplayManifest, Simulation, TrackDef, STATE_LEN};
use wasm_bindgen::prelude::*;

fn js_err(e: String) -> JsValue {
    JsValue::from_str(&e)
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
    #[wasm_bindgen(constructor)]
    pub fn new(track_json: &str, profile_json: &str, seed: u32) -> Result<Lab, JsValue> {
        let track = TrackDef::from_json(track_json).map_err(js_err)?;
        let profile: ProfileFile =
            serde_json::from_str(profile_json).map_err(|e| js_err(format!("perfil inválido: {e}")))?;
        let sim = Simulation::new(&track, &profile, seed as u64).map_err(js_err)?;
        Ok(Lab { sim, autopilot: Autopilot::default(), state: vec![0.0; STATE_LEN] })
    }

    /// Avanza un tick con la entrada dada.
    pub fn step(&mut self, steer: i8, throttle: u8, brake: u8, flags: u8) {
        self.sim.step(InputFrame { steer, throttle, brake, flags });
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
            out.extend_from_slice(&[s.pos.x, s.pos.y, s.pos.z, s.right.x, s.right.y, s.right.z, s.up.x, s.up.y, s.up.z, s.width]);
        }
        out
    }

    pub fn ground_y(&self) -> f32 {
        self.sim.track().ground_y
    }

    /// Replay de la sesión completa (JSON del manifest).
    ///
    /// # Errors
    ///
    /// Devuelve el mensaje de serialización si el replay no puede codificarse.
    pub fn replay_json(&self) -> Result<String, JsValue> {
        serde_json::to_string(&self.sim.replay()).map_err(|e| js_err(e.to_string()))
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
#[wasm_bindgen]
pub fn profile_schema() -> String {
    profile::schema_json()
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

/// Re-simula un replay y devuelve el informe (JSON de `VerifyReport`).
#[wasm_bindgen]
pub fn verify_replay(track_json: &str, replay_json: &str) -> Result<String, JsValue> {
    let track = TrackDef::from_json(track_json).map_err(js_err)?;
    let manifest = ReplayManifest::from_json(replay_json).map_err(js_err)?;
    let report = sim_core::verify_replay(&manifest, &track).map_err(js_err)?;
    serde_json::to_string(&report).map_err(|e| js_err(e.to_string()))
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
    let fields: Vec<_> = STATE_FIELDS.iter().enumerate().map(|(index, name)| {
        serde_json::json!({"name": name, "type": "f32", "offset": index * size_of::<f32>()})
    }).collect();
    serde_json::json!({"version": 1, "stride": STATE_LEN * size_of::<f32>(), "fields": fields}).to_string()
}
