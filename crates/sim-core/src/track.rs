//! Mini compilador de pistas para S0.
//!
//! Toma puntos de control hechos a mano y produce el eje re-muestreado, la coordenada
//! de distancia `s`, las superficies y las mallas de colisión y render.
//! Es un antecesor directo del `track-compiler` real: misma salida conceptual, otra entrada.

use crate::hash::Fnv64;
use crate::mathx::{cos, lerp, norm, sin, Vec3, DEG};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
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

#[derive(Clone, Debug, Serialize, Deserialize)]
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

/// Etiquetas de superficie. La física usa el agarre; el render, el color.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Surface {
    Asphalt = 0,
    KerbA = 1,
    KerbB = 2,
    Shoulder = 3,
    Wall = 4,
    StartLine = 5,
}

impl Surface {
    pub fn grip(self) -> f32 {
        match self {
            Surface::Asphalt | Surface::StartLine => 1.0,
            Surface::KerbA | Surface::KerbB => 0.92,
            Surface::Shoulder => 0.72,
            Surface::Wall => 0.5,
        }
    }

    pub fn from_u8(v: u8) -> Surface {
        match v {
            1 => Surface::KerbA,
            2 => Surface::KerbB,
            3 => Surface::Shoulder,
            4 => Surface::Wall,
            5 => Surface::StartLine,
            _ => Surface::Asphalt,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Sample {
    pub pos: Vec3,
    /// Tangente unitaria en el sentido de marcha.
    pub tangent: Vec3,
    /// Derecha de la calzada, ya con peralte.
    pub right: Vec3,
    /// Normal de la calzada, ya con peralte.
    pub up: Vec3,
    pub width: f32,
    /// Coordenada de distancia a lo largo del eje, en metros.
    pub s: f32,
    /// Curvatura con signo (1/m). Positiva = curva a la derecha.
    pub curvature: f32,
}

/// Malla con vértices sin compartir entre caras (cada triángulo tiene su etiqueta).
#[derive(Clone, Debug, Default)]
pub struct MeshPart {
    pub positions: Vec<f32>,
    pub surfaces: Vec<u8>,
    pub indices: Vec<u32>,
}

impl MeshPart {
    fn quad(&mut self, a: Vec3, b: Vec3, c: Vec3, d: Vec3, surface: Surface) {
        // a-b-c-d en orden antihorario visto desde la cara visible.
        let base = (self.positions.len() / 3) as u32;
        for p in [a, b, c, d] {
            self.positions.extend_from_slice(&[p.x, p.y, p.z]);
            self.surfaces.push(surface as u8);
        }
        self.indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    pub fn vertices(&self) -> Vec<Vec3> {
        self.positions.chunks_exact(3).map(|c| Vec3::new(c[0], c[1], c[2])).collect()
    }

    pub fn triangles(&self) -> Vec<[u32; 3]> {
        self.indices.chunks_exact(3).map(|c| [c[0], c[1], c[2]]).collect()
    }

    /// Superficie de un triángulo (todos sus vértices comparten etiqueta).
    pub fn triangle_surface(&self, tri: usize) -> Surface {
        let v = self.indices.get(tri * 3).copied().unwrap_or(0) as usize;
        Surface::from_u8(self.surfaces.get(v).copied().unwrap_or(0))
    }
}

#[derive(Clone, Debug)]
pub struct CompiledTrack {
    pub id: String,
    pub name: String,
    pub closed: bool,
    pub spacing: f32,
    pub samples: Vec<Sample>,
    pub length: f32,
    /// Checkpoints en metros sobre `s`, el primero es la meta (s = 0).
    pub checkpoints: Vec<f32>,
    /// Superficie conducible: calzada, pianos, banquinas.
    pub drivable: MeshPart,
    /// Solo render: la colisión de los muros usa `wall_boxes`.
    pub walls: MeshPart,
    /// Muros sólidos para colisión. Una malla delgada deja que el auto penetre y
    /// salga empujado hacia arriba por la tapa; una caja se resuelve hacia la pista.
    pub wall_boxes: Vec<WallBox>,
    /// Solo render: línea central discontinua.
    pub decals: MeshPart,
    pub ground_y: f32,
    pub hash: u64,
}

const KERB_W: f32 = 1.0;
const SHOULDER_W: f32 = 3.5;
const WALL_H: f32 = 1.2;
const WALL_T: f32 = 1.0;

/// Caja orientada: centro, semiejes (grosor, alto, largo) y rotación.
#[derive(Clone, Copy, Debug)]
pub struct WallBox {
    pub center: Vec3,
    pub half_extents: Vec3,
    pub rotation: crate::mathx::Quat,
}

impl TrackDef {
    pub fn from_json(s: &str) -> Result<Self, String> {
        serde_json::from_str(s).map_err(|e| format!("pista inválida: {e}"))
    }

    pub fn compile(&self) -> Result<CompiledTrack, String> {
        if self.points.len() < 4 {
            return Err("la pista necesita al menos 4 puntos de control".into());
        }
        if !self.closed {
            return Err("S0 solo admite pistas cerradas".into());
        }
        let n = self.points.len();
        let p = |i: isize| -> [f32; 5] {
            let c = &self.points[i.rem_euclid(n as isize) as usize];
            [c.x, c.y, c.z, c.width.unwrap_or(self.default_width), c.bank]
        };

        // 1. Polilínea densa con Catmull-Rom uniforme cerrado.
        const SUB: usize = 48;
        let mut dense: Vec<[f32; 5]> = Vec::with_capacity(n * SUB + 1);
        for i in 0..n as isize {
            let (p0, p1, p2, p3) = (p(i - 1), p(i), p(i + 1), p(i + 2));
            for k in 0..SUB {
                let t = k as f32 / SUB as f32;
                let (t2, t3) = (t * t, t * t * t);
                let mut out = [0.0f32; 5];
                for c in 0..5 {
                    out[c] = 0.5
                        * (2.0 * p1[c]
                            + (-p0[c] + p2[c]) * t
                            + (2.0 * p0[c] - 5.0 * p1[c] + 4.0 * p2[c] - p3[c]) * t2
                            + (-p0[c] + 3.0 * p1[c] - 3.0 * p2[c] + p3[c]) * t3);
                }
                dense.push(out);
            }
        }
        dense.push(dense[0]);

        // 2. Longitud acumulada y re-muestreo por distancia.
        let pos = |d: &[f32; 5]| Vec3::new(d[0], d[1], d[2]);
        let mut acc = vec![0.0f32; dense.len()];
        for i in 1..dense.len() {
            acc[i] = acc[i - 1] + (pos(&dense[i]) - pos(&dense[i - 1])).length();
        }
        let total = *acc.last().unwrap();
        let count = (total / self.sample_spacing).floor().max(8.0) as usize;
        let spacing = total / count as f32;
        let mut raw: Vec<[f32; 5]> = Vec::with_capacity(count);
        let mut j = 0usize;
        for k in 0..count {
            let target = k as f32 * spacing;
            while j + 1 < acc.len() - 1 && acc[j + 1] < target {
                j += 1;
            }
            let seg = (acc[j + 1] - acc[j]).max(1e-6);
            let t = ((target - acc[j]) / seg).clamp(0.0, 1.0);
            let mut out = [0.0f32; 5];
            for c in 0..5 {
                out[c] = lerp(dense[j][c], dense[j + 1][c], t);
            }
            raw.push(out);
        }

        // 3. Marcos locales: tangente, curvatura con signo, peralte.
        let m = raw.len();
        let at = |i: isize| pos(&raw[i.rem_euclid(m as isize) as usize]);
        let tangents: Vec<Vec3> = (0..m as isize).map(|i| norm(at(i + 1) - at(i - 1))).collect();
        let world_up = Vec3::Y;
        let flat_right = |t: Vec3| norm(t.cross(world_up));
        // Curvatura suavizada en una ventana, para que el peralte no cambie de lado bruscamente.
        const W: isize = 6;
        let curv: Vec<f32> = (0..m as isize)
            .map(|i| {
                let ta = tangents[(i - W).rem_euclid(m as isize) as usize];
                let tb = tangents[(i + W).rem_euclid(m as isize) as usize];
                let d = tb - ta;
                let r = flat_right(tangents[i as usize]);
                d.dot(r) / (2.0 * W as f32 * spacing)
            })
            .collect();

        let mut samples = Vec::with_capacity(m);
        for i in 0..m {
            let t = tangents[i];
            let r_flat = flat_right(t);
            let bank = raw[i][4].abs() * DEG;
            // Curva a la derecha (curvatura > 0): el borde derecho baja.
            let side = if curv[i] > 0.0 { -1.0 } else { 1.0 };
            let right = norm(r_flat * cos(bank) + world_up * (sin(bank) * side));
            let up = norm(right.cross(t));
            samples.push(Sample {
                pos: pos(&raw[i]),
                tangent: t,
                right,
                up,
                width: raw[i][3],
                s: i as f32 * spacing,
                curvature: curv[i],
            });
        }

        let min_y = samples.iter().map(|s| s.pos.y).fold(f32::INFINITY, f32::min);
        let ground_y = min_y - 0.6;

        // 4. Mallas.
        let mut drivable = MeshPart::default();
        let mut walls = MeshPart::default();
        let mut wall_boxes = Vec::with_capacity(m * 2);
        let mut decals = MeshPart::default();
        for i in 0..m {
            let a = &samples[i];
            let b = &samples[(i + 1) % m];
            let edge = |s: &Sample, off: f32| s.pos + s.right * off;
            let hw = |s: &Sample| s.width * 0.5;

            // Franja entre dos desplazamientos laterales (lo < hi), cara hacia arriba.
            let strip = |part: &mut MeshPart, lo_a: f32, hi_a: f32, lo_b: f32, hi_b: f32, s: Surface| {
                part.quad(edge(a, lo_a), edge(a, hi_a), edge(b, hi_b), edge(b, lo_b), s);
            };

            // Calzada (la primera franja es la línea de meta).
            let road = if i == 0 { Surface::StartLine } else { Surface::Asphalt };
            strip(&mut drivable, -hw(a), hw(a), -hw(b), hw(b), road);
            let kerb = if (i / 2) % 2 == 0 { Surface::KerbA } else { Surface::KerbB };
            let (ka, kb) = (hw(a) + KERB_W, hw(b) + KERB_W);
            let (sa, sb) = (ka + SHOULDER_W, kb + SHOULDER_W);
            strip(&mut drivable, -ka, -hw(a), -kb, -hw(b), kerb);
            strip(&mut drivable, hw(a), ka, hw(b), kb, kerb);
            strip(&mut drivable, -sa, -ka, -sb, -kb, Surface::Shoulder);
            strip(&mut drivable, ka, sa, kb, sb, Surface::Shoulder);

            // Línea central discontinua (solo render), apenas sobre la calzada.
            if i % 6 < 3 && i != 0 {
                let lift = 0.02;
                let base = (decals.positions.len() / 3) as u32;
                for p in [edge(a, -0.15), edge(a, 0.15), edge(b, 0.15), edge(b, -0.15)] {
                    let q = p + a.up * lift;
                    decals.positions.extend_from_slice(&[q.x, q.y, q.z]);
                    decals.surfaces.push(Surface::KerbB as u8);
                }
                decals.indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
            }

            for side in [-1.0f32, 1.0] {
                // Muro: cara interior, tapa y cara exterior; baja hasta el suelo.
                let ea = edge(a, sa * side);
                let eb = edge(b, sb * side);
                let out_a = flat_right(a.tangent) * (WALL_T * side);
                let out_b = flat_right(b.tangent) * (WALL_T * side);
                let top = Vec3::Y * WALL_H;
                // Caja de colisión del tramo.
                let along = Vec3::new(eb.x - ea.x, 0.0, eb.z - ea.z);
                let seg_len = along.length();
                if seg_len > 1e-3 {
                    let d = along / seg_len;
                    let outward = norm(d.cross(Vec3::Y)) * side;
                    let top_y = ea.y.max(eb.y) + WALL_H;
                    let mid = (ea + eb) * 0.5;
                    let center = Vec3::new(mid.x, (ground_y + top_y) * 0.5, mid.z) + outward * (WALL_T * 0.5);
                    let right_axis = norm(d.cross(Vec3::Y));
                    wall_boxes.push(WallBox {
                        center,
                        half_extents: Vec3::new(WALL_T * 0.5, (top_y - ground_y) * 0.5, seg_len * 0.5 + 0.15),
                        rotation: crate::mathx::basis_to_quat(right_axis, Vec3::Y, d),
                    });
                }
                let ga = Vec3::new(ea.x, ground_y, ea.z);
                let gb = Vec3::new(eb.x, ground_y, eb.z);
                let (ta, tb) = (ea + top, eb + top);
                let (oa, ob) = (ta + out_a, tb + out_b);
                let (ga_o, gb_o) = (ga + out_a, gb + out_b);
                if side < 0.0 {
                    walls.quad(ga, gb, tb, ta, Surface::Wall);
                    walls.quad(ta, tb, ob, oa, Surface::Wall);
                    walls.quad(oa, ob, gb_o, ga_o, Surface::Wall);
                } else {
                    walls.quad(ga, ta, tb, gb, Surface::Wall);
                    walls.quad(ta, oa, ob, tb, Surface::Wall);
                    walls.quad(oa, ga_o, gb_o, ob, Surface::Wall);
                }
            }
        }

        let length = spacing * m as f32;
        let mut checkpoints: Vec<f32> = self
            .checkpoints
            .iter()
            .map(|f| (f.clamp(0.0, 0.999)) * length)
            .collect();
        checkpoints.sort_by(|a, b| a.total_cmp(b));
        if checkpoints.first().copied() != Some(0.0) {
            checkpoints.insert(0, 0.0);
        }
        checkpoints.dedup();

        let mut h = Fnv64::new();
        h.str("compiled-track/1").str(crate::COMPILER_VERSION).str(&self.id);
        for s in &samples {
            for v in [s.pos, s.right, s.up] {
                h.f32(v.x).f32(v.y).f32(v.z);
            }
            h.f32(s.width);
        }
        for c in &checkpoints {
            h.f32(*c);
        }

        Ok(CompiledTrack {
            id: self.id.clone(),
            name: self.name.clone(),
            closed: true,
            spacing,
            samples,
            length,
            checkpoints,
            drivable,
            walls,
            wall_boxes,
            decals,
            ground_y,
            hash: h.finish(),
        })
    }
}

impl CompiledTrack {
    pub fn sample(&self, i: isize) -> &Sample {
        let m = self.samples.len() as isize;
        &self.samples[i.rem_euclid(m) as usize]
    }

    /// Muestra más cercana buscando en una ventana alrededor de `hint`.
    pub fn nearest_near(&self, p: Vec3, hint: usize, window: isize) -> usize {
        let mut best = hint;
        let mut best_d = f32::INFINITY;
        for k in -window..=window {
            let i = (hint as isize + k).rem_euclid(self.samples.len() as isize) as usize;
            let d = (self.samples[i].pos - p).length_squared();
            if d < best_d {
                best_d = d;
                best = i;
            }
        }
        best
    }

    pub fn nearest_global(&self, p: Vec3) -> usize {
        let mut best = 0;
        let mut best_d = f32::INFINITY;
        for (i, s) in self.samples.iter().enumerate() {
            let d = (s.pos - p).length_squared();
            if d < best_d {
                best_d = d;
                best = i;
            }
        }
        best
    }

    /// Proyección continua de `p` sobre el eje, cerca de la muestra `i`. Devuelve `s` en [0, L).
    pub fn project_s(&self, p: Vec3, i: usize) -> f32 {
        let smp = &self.samples[i];
        let s = smp.s + (p - smp.pos).dot(smp.tangent);
        s.rem_euclid(self.length)
    }

    /// Curvatura máxima absoluta entre `s` y `s + ahead` metros.
    pub fn max_curvature_ahead(&self, i: usize, ahead: f32) -> f32 {
        let steps = (ahead / self.spacing).ceil() as isize;
        (0..=steps).map(|k| self.sample(i as isize + k).curvature.abs()).fold(0.0, f32::max)
    }

    pub fn track_hash_hex(&self) -> String {
        crate::hash::to_hex(self.hash)
    }
}

/// Distancia con signo de `a` a `b` sobre un circuito de longitud `l`, en [-l/2, l/2).
pub fn wrap_delta(a: f32, b: f32, l: f32) -> f32 {
    let d = (b - a).rem_euclid(l);
    if d >= l * 0.5 {
        d - l
    } else {
        d
    }
}
