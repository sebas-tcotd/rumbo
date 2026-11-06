//! Source track data; vertices retain per-triangle height and identity.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ControlPoint {
    pub x: f32,
    pub z: f32,
    #[serde(default)]
    pub y: f32,
    /// Ancho de la calzada en metros. Si falta, se usa `defaultWidth`.
    #[serde(default)]
    pub width: Option<f32>,
    /// Peralte en grados. Siempre se inclina hacia el interior de la curva.
    #[serde(default)]
    pub bank: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackDef {
    pub id: String,
    pub name: String,
    pub closed: bool,
    #[serde(default = "default_spacing")]
    pub sample_spacing: f32,
    #[serde(default = "default_width")]
    pub default_width: f32,
    /// Posiciones de checkpoints como fracción de la longitud (el 0 es la meta).
    #[serde(default = "default_checkpoints")]
    pub checkpoints: Vec<f32>,
    pub points: Vec<ControlPoint>,
    /// S3: superficie y muros ya compilados (calles, cruces y calles laterales). Si está, la
    /// cinta de `points` solo sirve de referencia para el cronómetro y el piloto automático.
    #[serde(default)]
    pub network: Option<NetworkDef>,
}

/// Red conducible compilada por `track-compiler` (S3).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NetworkDef {
    pub vertices: Vec<[f32; 3]>,
    /// Triángulos antihorarios vistos desde arriba.
    pub triangles: Vec<[u32; 3]>,
    /// Superficie de cada triángulo (`Surface`).
    pub surfaces: Vec<u8>,
    /// Muros: polilíneas a nivel de la calzada, con la zona conducible a la izquierda.
    pub walls: Vec<Vec<[f32; 3]>>,
    /// Barreras de carrera que cierran las calles laterales (sólidas como un muro).
    #[serde(default)]
    pub barriers: Vec<BarrierDef>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BarrierDef {
    pub line: Vec<[f32; 3]>,
    /// +1: las flechas apuntan del primer punto al último; -1: al revés.
    pub arrow: f32,
}

fn default_spacing() -> f32 {
    2.0
}
fn default_width() -> f32 {
    12.0
}
fn default_checkpoints() -> Vec<f32> {
    vec![0.0, 0.33, 0.66]
}

impl TrackDef {
    /// Parse a legacy JSON source track, accepting an absent network.
    /// # Errors
    /// Returns the JSON parser diagnostic for invalid source data.
    pub fn from_json(s: &str) -> Result<Self, String> {
        serde_json::from_str(s).map_err(|e| format!("pista inválida: {e}"))
    }
}
