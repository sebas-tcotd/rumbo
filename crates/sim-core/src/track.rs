//! Mini compilador de pistas para S0.
//!
//! Toma puntos de control hechos a mano y produce el eje re-muestreado, la coordenada
//! de distancia `s`, las superficies y las mallas de colisión y render.
//! Es un antecesor directo del `track-compiler` real: misma salida conceptual, otra entrada.

use crate::hash::Fnv64;
use crate::mathx::{cos, lerp, norm, sin, Vec3, DEG};

pub use track_format::domain::track::{BarrierDef, ControlPoint, NetworkDef, TrackDef};

/// Barrera compilada: puntos a nivel de la calzada y sentido de las flechas (solo render).
#[derive(Clone, Debug)]
pub struct Barrier {
    pub line: Vec<Vec3>,
    pub arrow: f32,
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
    /// Calibración experimental: el arcén alcanza su pico a mayor deslizamiento.
    pub fn slip_peak_multiplier(self) -> f32 {
        match self {
            Surface::Shoulder => 1.4,
            Surface::KerbA | Surface::KerbB => 1.1,
            _ => 1.0,
        }
    }

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
    /// Curvatura sin signo medida en una ventana corta (±4 muestras). La de arriba se
    /// promedia en ±12 m para el peralte y subestima las esquinas cortas de una ciudad.
    pub curvature_tight: f32,
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
        self.indices
            .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    pub fn vertices(&self) -> Vec<Vec3> {
        self.positions
            .chunks_exact(3)
            .map(|c| Vec3::new(c[0], c[1], c[2]))
            .collect()
    }

    pub fn triangles(&self) -> Vec<[u32; 3]> {
        self.indices
            .chunks_exact(3)
            .map(|c| [c[0], c[1], c[2]])
            .collect()
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
    /// Sprint: posición de la línea de salida y de la llegada sobre `s` (0 en circuitos).
    pub start_s: f32,
    pub finish_s: f32,
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
    /// La superficie viene de una red compilada (S3) y no de la cinta.
    pub network: bool,
    /// Barreras de carrera (su colisión ya está en `wall_boxes`).
    pub barriers: Vec<Barrier>,
}

const KERB_W: f32 = 1.0;
const SHOULDER_W: f32 = 3.5;
const WALL_H: f32 = 1.2;
/// Sprint: carrerilla entre el inicio de la pista y la línea de salida.
pub const OPEN_START_M: f32 = 24.0;
/// Sprint: zona de frenado entre la llegada y el muro final (frenar desde ~200 km/h).
pub const OPEN_RUNOFF_M: f32 = 120.0;
const WALL_T: f32 = 1.0;

/// Caja orientada: centro, semiejes (grosor, alto, largo) y rotación.
#[derive(Clone, Copy, Debug)]
pub struct WallBox {
    pub center: Vec3,
    pub half_extents: Vec3,
    pub rotation: crate::mathx::Quat,
}

/// Compile pure source data into runtime geometry.
/// # Errors
/// Rejects insufficient points, invalid sprints and inconsistent network triangles.
pub fn compile_track(def: &TrackDef) -> Result<CompiledTrack, String> {
    if def.points.len() < 4 {
        return Err("la pista necesita al menos 4 puntos de control".into());
    }
    // Las pistas abiertas (sprint) recorren los mismos pasos sin dar la vuelta en los
    // extremos. La rama cerrada conserva exactamente la aritmética de S0.
    let closed = def.closed;
    let n = def.points.len();
    let p = |i: isize| -> [f32; 5] {
        let idx = if closed {
            i.rem_euclid(n as isize)
        } else {
            i.clamp(0, n as isize - 1)
        };
        let c = &def.points[idx as usize];
        [c.x, c.y, c.z, c.width.unwrap_or(def.default_width), c.bank]
    };

    // 1. Polilínea densa con Catmull-Rom uniforme.
    const SUB: usize = 48;
    let segments = if closed { n } else { n - 1 };
    let mut dense: Vec<[f32; 5]> = Vec::with_capacity(n * SUB + 1);
    for i in 0..segments as isize {
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
    if closed {
        dense.push(dense[0]);
    } else {
        dense.push(p(n as isize - 1));
    }

    // 2. Longitud acumulada y re-muestreo por distancia.
    let pos = |d: &[f32; 5]| Vec3::new(d[0], d[1], d[2]);
    let mut acc = vec![0.0f32; dense.len()];
    for i in 1..dense.len() {
        acc[i] = acc[i - 1] + (pos(&dense[i]) - pos(&dense[i - 1])).length();
    }
    let total = *acc.last().unwrap();
    let count = (total / def.sample_spacing).floor().max(8.0) as usize;
    let spacing = total / count as f32;
    let mut raw: Vec<[f32; 5]> = Vec::with_capacity(count + 1);
    let mut j = 0usize;
    // Cerrada: la última muestra se une con la primera. Abierta: incluye el extremo final.
    let last = if closed { count } else { count + 1 };
    for k in 0..last {
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
    let wrap = |i: isize| {
        if closed {
            i.rem_euclid(m as isize) as usize
        } else {
            i.clamp(0, m as isize - 1) as usize
        }
    };
    let at = |i: isize| pos(&raw[wrap(i)]);
    let tangents: Vec<Vec3> = (0..m as isize)
        .map(|i| norm(at(i + 1) - at(i - 1)))
        .collect();
    let world_up = Vec3::Y;
    let flat_right = |t: Vec3| norm(t.cross(world_up));
    // Curvatura suavizada en una ventana, para que el peralte no cambie de lado bruscamente.
    const W: isize = 6;
    let curv: Vec<f32> = (0..m as isize)
        .map(|i| {
            let (ia, ib) = (wrap(i - W), wrap(i + W));
            let (ta, tb) = (tangents[ia], tangents[ib]);
            let d = tb - ta;
            let r = flat_right(tangents[i as usize]);
            let span = if closed {
                2.0 * W as f32
            } else {
                (ib - ia).max(1) as f32
            };
            d.dot(r) / (span * spacing)
        })
        .collect();

    const WT: isize = 2;
    let tight: Vec<f32> = (0..m as isize)
        .map(|i| {
            let (ia, ib) = (wrap(i - WT), wrap(i + WT));
            let span = if closed {
                2.0 * WT as f32
            } else {
                (ib - ia).max(1) as f32
            };
            (tangents[ib] - tangents[ia]).length() / (span * spacing)
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
            curvature_tight: tight[i],
        });
    }

    let mut min_y = samples
        .iter()
        .map(|s| s.pos.y)
        .fold(f32::INFINITY, f32::min);
    if let Some(net) = &def.network {
        min_y = net.vertices.iter().map(|v| v[1]).fold(min_y, f32::min);
    }
    let ground_y = min_y - 0.6;

    let length = if closed {
        spacing * m as f32
    } else {
        spacing * (m - 1) as f32
    };
    // Sprint: líneas de salida y llegada separadas de los extremos para tener carrerilla.
    let (start_s, finish_s) = if closed {
        (0.0, 0.0)
    } else {
        (
            OPEN_START_M,
            (length - OPEN_RUNOFF_M)
                .max(OPEN_START_M + 50.0)
                .min(length - 10.0),
        )
    };
    if !closed && finish_s <= start_s {
        return Err(format!(
            "el sprint mide {length:.0} m: hacen falta al menos {:.0} m",
            OPEN_START_M + 60.0
        ));
    }
    let line_index = |s_line: f32| (s_line / spacing).round() as usize;
    let lines: Vec<usize> = if closed {
        vec![0]
    } else {
        vec![line_index(start_s), line_index(finish_s)]
    };

    // 4. Mallas.
    let mut drivable = MeshPart::default();
    let mut walls = MeshPart::default();
    let mut wall_boxes = Vec::with_capacity(m * 2);
    let mut decals = MeshPart::default();
    let quads = if def.network.is_some() {
        0
    } else if closed {
        m
    } else {
        m - 1
    };
    for i in 0..quads {
        let a = &samples[i];
        let b = &samples[(i + 1) % m];
        let edge = |s: &Sample, off: f32| s.pos + s.right * off;
        let hw = |s: &Sample| s.width * 0.5;

        // Franja entre dos desplazamientos laterales (lo < hi), cara hacia arriba.
        let strip =
            |part: &mut MeshPart, lo_a: f32, hi_a: f32, lo_b: f32, hi_b: f32, s: Surface| {
                part.quad(
                    edge(a, lo_a),
                    edge(a, hi_a),
                    edge(b, hi_b),
                    edge(b, lo_b),
                    s,
                );
            };

        // Calzada (la franja de cada línea de salida o meta se marca aparte).
        let road = if lines.contains(&i) {
            Surface::StartLine
        } else {
            Surface::Asphalt
        };
        strip(&mut drivable, -hw(a), hw(a), -hw(b), hw(b), road);
        let kerb = if (i / 2) % 2 == 0 {
            Surface::KerbA
        } else {
            Surface::KerbB
        };
        let (ka, kb) = (hw(a) + KERB_W, hw(b) + KERB_W);
        let (sa, sb) = (ka + SHOULDER_W, kb + SHOULDER_W);
        strip(&mut drivable, -ka, -hw(a), -kb, -hw(b), kerb);
        strip(&mut drivable, hw(a), ka, hw(b), kb, kerb);
        strip(&mut drivable, -sa, -ka, -sb, -kb, Surface::Shoulder);
        strip(&mut drivable, ka, sa, kb, sb, Surface::Shoulder);

        // Línea central discontinua (solo render), apenas sobre la calzada.
        if i % 6 < 3 && !lines.contains(&i) {
            let lift = 0.02;
            let base = (decals.positions.len() / 3) as u32;
            for p in [edge(a, -0.15), edge(a, 0.15), edge(b, 0.15), edge(b, -0.15)] {
                let q = p + a.up * lift;
                decals.positions.extend_from_slice(&[q.x, q.y, q.z]);
                decals.surfaces.push(Surface::KerbB as u8);
            }
            decals
                .indices
                .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
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
                let center =
                    Vec3::new(mid.x, (ground_y + top_y) * 0.5, mid.z) + outward * (WALL_T * 0.5);
                let right_axis = norm(d.cross(Vec3::Y));
                wall_boxes.push(WallBox {
                    center,
                    half_extents: Vec3::new(
                        WALL_T * 0.5,
                        (top_y - ground_y) * 0.5,
                        seg_len * 0.5 + 0.15,
                    ),
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

    if let Some(net) = &def.network {
        build_network(
            net,
            &samples,
            &lines,
            ground_y,
            &mut drivable,
            &mut walls,
            &mut wall_boxes,
            &mut decals,
        )?;
    }

    // Muros transversales al principio y al final de un sprint.
    if !closed && def.network.is_none() {
        for (idx, dir) in [(0usize, -1.0f32), (m - 1, 1.0)] {
            let smp = &samples[idx];
            let half = smp.width * 0.5 + KERB_W + SHOULDER_W + WALL_T;
            let fwd = norm(Vec3::new(smp.tangent.x, 0.0, smp.tangent.z));
            let right_axis = norm(fwd.cross(Vec3::Y));
            let top_y = smp.pos.y + WALL_H;
            let base = smp.pos + fwd * (WALL_T * 0.5 * dir);
            wall_boxes.push(WallBox {
                center: Vec3::new(base.x, (ground_y + top_y) * 0.5, base.z),
                half_extents: Vec3::new(half, (top_y - ground_y) * 0.5, WALL_T * 0.5),
                rotation: crate::mathx::basis_to_quat(right_axis, Vec3::Y, fwd),
            });
            // Cara hacia la pista y tapa, para el render.
            let (l, r) = (smp.pos - right_axis * half, smp.pos + right_axis * half);
            let (lg, rg) = (Vec3::new(l.x, ground_y, l.z), Vec3::new(r.x, ground_y, r.z));
            let (lt, rt) = (l + Vec3::Y * WALL_H, r + Vec3::Y * WALL_H);
            let back = fwd * (WALL_T * dir);
            if dir < 0.0 {
                walls.quad(lg, rg, rt, lt, Surface::Wall);
            } else {
                walls.quad(rg, lg, lt, rt, Surface::Wall);
            }
            walls.quad(lt, rt, rt + back, lt + back, Surface::Wall);
        }
    }

    let checkpoints: Vec<f32> = if closed {
        let mut c: Vec<f32> = def
            .checkpoints
            .iter()
            .map(|f| (f.clamp(0.0, 0.999)) * length)
            .collect();
        c.sort_by(|a, b| a.total_cmp(b));
        if c.first().copied() != Some(0.0) {
            c.insert(0, 0.0);
        }
        c.dedup();
        c
    } else {
        // Salida, intermedios (fracciones del tramo entre salida y llegada) y llegada.
        let mut c = vec![start_s];
        let mut mids: Vec<f32> = def
            .checkpoints
            .iter()
            .filter(|f| **f > 0.0 && **f < 1.0)
            .map(|f| start_s + f * (finish_s - start_s))
            .collect();
        mids.sort_by(|a, b| a.total_cmp(b));
        c.extend(mids);
        c.push(finish_s);
        c.dedup();
        c
    };

    let mut h = Fnv64::new();
    h.str_u32_len("compiled-track/1")
        .str_u32_len(crate::COMPILER_VERSION)
        .str_u32_len(&def.id);
    if !closed {
        h.str_u32_len("open"); // solo en pistas abiertas: el hash de las cerradas no cambia
    }
    for s in &samples {
        for v in [s.pos, s.right, s.up] {
            h.f32(v.x).f32(v.y).f32(v.z);
        }
        h.f32(s.width);
    }
    for c in &checkpoints {
        h.f32(*c);
    }
    if let Some(net) = &def.network {
        h.str_u32_len("network");
        for v in &net.vertices {
            h.f32(v[0]).f32(v[1]).f32(v[2]);
        }
        for (t, s) in net.triangles.iter().zip(&net.surfaces) {
            h.u32(t[0]).u32(t[1]).u32(t[2]).u32(*s as u32);
        }
        for w in &net.walls {
            h.u32(w.len() as u32);
            for v in w {
                h.f32(v[0]).f32(v[1]).f32(v[2]);
            }
        }
        if !net.barriers.is_empty() {
            h.str_u32_len("barriers");
            for b in &net.barriers {
                h.u32(b.line.len() as u32).f32(b.arrow);
                for v in &b.line {
                    h.f32(v[0]).f32(v[1]).f32(v[2]);
                }
            }
        }
    }

    Ok(CompiledTrack {
        id: def.id.clone(),
        name: def.name.clone(),
        closed,
        start_s,
        finish_s,
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
        network: def.network.is_some(),
        barriers: def
            .network
            .iter()
            .flat_map(|n| n.barriers.iter())
            .map(|b| Barrier {
                line: b.line.iter().map(|p| Vec3::new(p[0], p[1], p[2])).collect(),
                arrow: b.arrow,
            })
            .collect(),
    })
}

/// Muro sólido de `a` a `b` con la pista a la izquierda: caja de colisión y caras para el render.
fn wall_segment(
    a: Vec3,
    b: Vec3,
    ground_y: f32,
    walls: &mut MeshPart,
    wall_boxes: &mut Vec<WallBox>,
) {
    let along = Vec3::new(b.x - a.x, 0.0, b.z - a.z);
    let seg_len = along.length();
    if seg_len < 1e-3 {
        return;
    }
    let d = along / seg_len;
    let outward = norm(d.cross(Vec3::Y));
    let top_y = a.y.max(b.y) + WALL_H;
    let mid = (a + b) * 0.5;
    wall_boxes.push(WallBox {
        center: Vec3::new(mid.x, (ground_y + top_y) * 0.5, mid.z) + outward * (WALL_T * 0.5),
        half_extents: Vec3::new(WALL_T * 0.5, (top_y - ground_y) * 0.5, seg_len * 0.5 + 0.15),
        rotation: crate::mathx::basis_to_quat(outward, Vec3::Y, d),
    });
    let out = outward * WALL_T;
    let top = Vec3::Y * WALL_H;
    let (ga, gb) = (Vec3::new(a.x, ground_y, a.z), Vec3::new(b.x, ground_y, b.z));
    let (ta, tb) = (a + top, b + top);
    let (oa, ob) = (ta + out, tb + out);
    walls.quad(ga, ta, tb, gb, Surface::Wall);
    walls.quad(ta, oa, ob, tb, Surface::Wall);
    walls.quad(oa, ga + out, gb + out, ob, Surface::Wall);
}

/// S3: superficie, muros y marcas a partir de la red compilada.
#[allow(clippy::too_many_arguments)]
fn build_network(
    net: &NetworkDef,
    samples: &[Sample],
    lines: &[usize],
    ground_y: f32,
    drivable: &mut MeshPart,
    walls: &mut MeshPart,
    wall_boxes: &mut Vec<WallBox>,
    decals: &mut MeshPart,
) -> Result<(), String> {
    if net.surfaces.len() != net.triangles.len() {
        return Err("red inválida: una superficie por triángulo".into());
    }
    let nv = net.vertices.len() as u32;
    let v = |i: u32| {
        let p = net.vertices[i as usize];
        Vec3::new(p[0], p[1], p[2])
    };
    for (t, &s) in net.triangles.iter().zip(&net.surfaces) {
        if t.iter().any(|&i| i >= nv) {
            return Err("red inválida: triángulo con un vértice que no existe".into());
        }
        let base = (drivable.positions.len() / 3) as u32;
        for &i in t {
            let p = v(i);
            drivable.positions.extend_from_slice(&[p.x, p.y, p.z]);
            drivable.surfaces.push(Surface::from_u8(s) as u8);
        }
        drivable
            .indices
            .extend_from_slice(&[base, base + 1, base + 2]);
    }
    for w in &net.walls {
        for seg in w.windows(2) {
            let (a, b) = (
                Vec3::new(seg[0][0], seg[0][1], seg[0][2]),
                Vec3::new(seg[1][0], seg[1][1], seg[1][2]),
            );
            wall_segment(a, b, ground_y, walls, wall_boxes);
        }
    }
    // Barreras: misma colisión que un muro; el render las dibuja aparte, con flechas.
    let mut hidden = MeshPart::default();
    for b in &net.barriers {
        for seg in b.line.windows(2) {
            let (a, c) = (
                Vec3::new(seg[0][0], seg[0][1], seg[0][2]),
                Vec3::new(seg[1][0], seg[1][1], seg[1][2]),
            );
            wall_segment(a, c, ground_y, &mut hidden, wall_boxes);
        }
    }
    // Línea de salida o meta pintada sobre la calzada (solo render).
    for &i in lines {
        let smp = &samples[i.min(samples.len() - 1)];
        let (hw, half_len, lift) = (smp.width * 0.5, 0.6, smp.up * 0.03);
        let along = smp.tangent * half_len;
        let base = (decals.positions.len() / 3) as u32;
        for p in [
            smp.pos - smp.right * hw - along,
            smp.pos - smp.right * hw + along,
            smp.pos + smp.right * hw + along,
            smp.pos + smp.right * hw - along,
        ] {
            let q = p + lift;
            decals.positions.extend_from_slice(&[q.x, q.y, q.z]);
            decals.surfaces.push(Surface::StartLine as u8);
        }
        decals
            .indices
            .extend_from_slice(&[base, base + 2, base + 1, base, base + 3, base + 2]);
    }
    Ok(())
}

impl CompiledTrack {
    pub fn sample(&self, i: isize) -> &Sample {
        let m = self.samples.len() as isize;
        let idx = if self.closed {
            i.rem_euclid(m)
        } else {
            i.clamp(0, m - 1)
        };
        &self.samples[idx as usize]
    }

    /// Muestra más cercana buscando en una ventana alrededor de `hint`.
    pub fn nearest_near(&self, p: Vec3, hint: usize, window: isize) -> usize {
        let mut best = hint;
        let mut best_d = f32::INFINITY;
        let m = self.samples.len() as isize;
        for k in -window..=window {
            let j = hint as isize + k;
            if !self.closed && (j < 0 || j >= m) {
                continue;
            }
            let i = j.rem_euclid(m) as usize;
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
        if self.closed {
            s.rem_euclid(self.length)
        } else {
            s.clamp(0.0, self.length)
        }
    }

    /// Curvatura máxima absoluta entre `s` y `s + ahead` metros.
    pub fn max_curvature_ahead(&self, i: usize, ahead: f32) -> f32 {
        let steps = (ahead / self.spacing).ceil() as isize;
        (0..=steps)
            .map(|k| {
                let s = self.sample(i as isize + k);
                s.curvature.abs().max(s.curvature_tight)
            })
            .fold(0.0, f32::max)
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
