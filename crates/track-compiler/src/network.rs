//! S3 · Red conducible: las calles del recorrido, sus cruces y las calles laterales.
//!
//! La pista de S2 era una sola cinta. Aquí la superficie se arma por piezas que comparten
//! sus bordes exactos:
//!
//! - **Corredores:** cada tramo de calle entre dos cruces (o de un cruce a un tramo corto
//!   que se cierra con un muro). Calzada al centro y vereda a cada lado.
//! - **Cruces:** cada calle llega al cruce con su dirección y su ancho. Entre dos calles
//!   vecinas, el borde de una y el de la otra se cortan en una esquina; la esquina de la
//!   calzada se redondea con un radio de vereda y la de la vereda (línea de fachada) queda
//!   en ángulo. Cada calle se corta en su *boca*: el punto hasta donde llega el cruce.
//!   La boca es un segmento perpendicular a la calle, y es exactamente la primera sección
//!   del corredor: así cruce y calle comparten vértices y no quedan huecos ni escalones.
//!
//! Plano local (este, norte). "Izquierda" es la normal girada +90° (antihorario).
//! Los triángulos salen en sentido antihorario vistos desde arriba, y los muros con la
//! zona conducible a su izquierda.

use crate::centerline::{gaussian, resample};
use crate::diagnostics::{Diagnostic, Severity};
use crate::geo::LocalFrame;
use crate::graph::RoadGraph;
use crate::routing::ResolvedRoute;
use crate::trackgen::Status;
use std::collections::{BTreeMap, BTreeSet};

/// Códigos de superficie de sim-core (`track::Surface`).
pub const ASPHALT: u8 = 0;
pub const SIDEWALK: u8 = 3;

#[derive(Clone, Copy, Debug)]
pub struct NetworkOptions {
    /// Ancho de la vereda a cada lado (m). Detrás va el muro (la línea de fachadas).
    pub sidewalk_m: f64,
    /// Radio de la esquina de la calzada en los cruces (m).
    pub curb_radius_m: f64,
    /// Largo de las calles laterales que no llevan a otro cruce del recorrido (m).
    pub stub_m: f64,
    pub spacing_m: f64,
    pub smooth_sigma_m: f64,
    pub min_width_m: f64,
    pub max_width_m: f64,
    /// Dos calles a partir de este ángulo se tratan como una misma vía que sigue derecho.
    pub straight_deg: f64,
    pub arc_segments: usize,
}

impl Default for NetworkOptions {
    fn default() -> Self {
        Self {
            sidewalk_m: 2.5,
            curb_radius_m: 5.0,
            stub_m: 30.0,
            spacing_m: 2.0,
            smooth_sigma_m: 4.0,
            min_width_m: 5.0,
            max_width_m: 16.0,
            straight_deg: 165.0,
            arc_segments: 6,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Junction {
    pub vertex: usize,
    pub en: [f64; 2],
    /// "T", "cruce", "Y" o "N brazos".
    pub kind: String,
    pub arms: usize,
    pub min_angle_deg: f64,
    pub status: Status,
    pub reason: Option<String>,
    /// Ids de las aristas de cada brazo, en sentido antihorario.
    pub arm_edges: Vec<String>,
}

#[derive(Clone, Debug, Default)]
pub struct Network {
    /// Vértices en el plano local, compartidos entre piezas.
    pub vertices: Vec<[f64; 2]>,
    /// Triángulos (antihorarios) con su superficie.
    pub triangles: Vec<([u32; 3], u8)>,
    /// Muros: polilíneas con la zona conducible a la izquierda.
    pub walls: Vec<Vec<[f64; 2]>>,
    pub junctions: Vec<Junction>,
    pub diagnostics: Vec<Diagnostic>,
    /// Largo total de calle conducible (m), sin contar los cruces.
    pub street_length_m: f64,
    /// Línea de referencia del circuito (para el cronómetro y el piloto automático): sigue el
    /// eje de cada calle y, en los cruces donde dobla, un arco paralelo a la esquina.
    /// Cerrada, a paso de 1 m, empezando junto al primer punto del recorrido.
    pub reference: Vec<RefPoint>,
}

#[derive(Clone, Copy, Debug)]
pub struct RefPoint {
    pub en: [f64; 2],
    pub width: f64,
    /// Arista del grafo de la que sale (para heredar atributos).
    pub edge: usize,
}

struct Corridor {
    /// Aristas en orden de recorrido y si van en su sentido de digitalización.
    edges: Vec<(usize, bool)>,
    start: usize,
    /// Cruce donde termina, si termina en uno de la red; si no, es una calle lateral corta.
    end: Option<usize>,
    on_route: bool,
    /// Eje suavizado a paso de 1 m, del cruce de inicio hacia el final.
    axis: Vec<[f64; 2]>,
    half: f64,
}

/// Esquina redondeada de un sector: tangentes en cada brazo, centro del arco, esquina de
/// la línea de fachadas y ángulo del sector (rad).
type Corner = ([f64; 2], [f64; 2], [f64; 2], [f64; 2], f64);

#[derive(Clone, Copy)]
struct Arm {
    corridor: usize,
    at_start: bool,
    d: [f64; 2],
    half: f64,
}

/// Geometría resuelta de un cruce: bocas por brazo y cadenas de borde por sector.
struct JunctionGeom {
    center: [f64; 2],
    arms: Vec<Arm>,
    /// Por brazo: [fachada derecha, calzada derecha, calzada izquierda, fachada izquierda].
    mouths: Vec<[[f64; 2]; 4]>,
    /// Por sector (brazo i → brazo i+1): borde de la calzada y línea de fachadas,
    /// ambos desde la boca del brazo i hasta la del i+1.
    inner: Vec<Vec<[f64; 2]>>,
    outer: Vec<Vec<[f64; 2]>>,
    setback: Vec<f64>,
    sector_deg: Vec<f64>,
    /// Por sector: centro del arco de la esquina, si la esquina se redondea.
    arc_center: Vec<Option<[f64; 2]>>,
}

fn add(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    [a[0] + b[0], a[1] + b[1]]
}
fn sub(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    [a[0] - b[0], a[1] - b[1]]
}
fn mul(a: [f64; 2], k: f64) -> [f64; 2] {
    [a[0] * k, a[1] * k]
}
fn dot(a: [f64; 2], b: [f64; 2]) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}
fn cross(a: [f64; 2], b: [f64; 2]) -> f64 {
    a[0] * b[1] - a[1] * b[0]
}
fn len(a: [f64; 2]) -> f64 {
    libm::hypot(a[0], a[1])
}
fn unit(a: [f64; 2]) -> [f64; 2] {
    let l = len(a).max(1e-12);
    [a[0] / l, a[1] / l]
}
/// Normal izquierda (giro de +90°).
fn left(d: [f64; 2]) -> [f64; 2] {
    [-d[1], d[0]]
}
fn angle(d: [f64; 2]) -> f64 {
    libm::atan2(d[1], d[0])
}

/// Intersección de las rectas p + d·t y q + e·u. `None` si son paralelas.
fn intersect(p: [f64; 2], d: [f64; 2], q: [f64; 2], e: [f64; 2]) -> Option<[f64; 2]> {
    let den = cross(d, e);
    if den.abs() < 1e-9 {
        return None;
    }
    let t = cross(sub(q, p), e) / den;
    Some(add(p, mul(d, t)))
}

/// Punto a una distancia `s` a lo largo de una polilínea (recortado a sus extremos).
fn point_at(poly: &[[f64; 2]], s: f64) -> [f64; 2] {
    let mut acc = 0.0;
    for w in poly.windows(2) {
        let l = len(sub(w[1], w[0]));
        if acc + l >= s && l > 1e-12 {
            return add(w[0], mul(sub(w[1], w[0]), (s - acc) / l));
        }
        acc += l;
    }
    *poly.last().unwrap()
}

fn poly_len(poly: &[[f64; 2]]) -> f64 {
    poly.windows(2).map(|w| len(sub(w[1], w[0]))).sum()
}

/// Tramo de una polilínea entre dos distancias.
fn slice(poly: &[[f64; 2]], a: f64, b: f64) -> Vec<[f64; 2]> {
    let mut out = vec![point_at(poly, a)];
    let mut acc = 0.0;
    for w in poly.windows(2) {
        acc += len(sub(w[1], w[0]));
        if acc > a + 1e-9 && acc < b - 1e-9 {
            out.push(w[1]);
        }
    }
    out.push(point_at(poly, b));
    out
}

/// Malla con vértices compartidos: el mismo punto (bit a bit) da el mismo índice.
#[derive(Default)]
struct MeshBuilder {
    vertices: Vec<[f64; 2]>,
    index: BTreeMap<(u64, u64), u32>,
    triangles: Vec<([u32; 3], u8)>,
}

impl MeshBuilder {
    fn vertex(&mut self, p: [f64; 2]) -> u32 {
        let key = (p[0].to_bits(), p[1].to_bits());
        if let Some(&i) = self.index.get(&key) {
            return i;
        }
        let i = self.vertices.len() as u32;
        self.vertices.push(p);
        self.index.insert(key, i);
        i
    }

    /// Triángulo antihorario; los degenerados se descartan.
    fn tri(&mut self, a: [f64; 2], b: [f64; 2], c: [f64; 2], surface: u8) {
        let area = cross(sub(b, a), sub(c, a));
        if area.abs() < 1e-6 {
            return;
        }
        let (ia, ib, ic) = (self.vertex(a), self.vertex(b), self.vertex(c));
        if ia == ib || ib == ic || ia == ic {
            return;
        }
        self.triangles.push((if area > 0.0 { [ia, ib, ic] } else { [ia, ic, ib] }, surface));
    }

    fn quad(&mut self, a: [f64; 2], b: [f64; 2], c: [f64; 2], d: [f64; 2], surface: u8) {
        self.tri(a, b, c, surface);
        self.tri(a, c, d, surface);
    }

    /// Banda entre dos cadenas que empiezan y terminan juntas en sus bocas. Triangulación en
    /// cremallera: avanza por la cadena cuyo próximo vértice está antes (en fracción de largo),
    /// sin crear vértices nuevos, para que la banda comparta exactamente los bordes vecinos.
    fn band(&mut self, inner: &[[f64; 2]], outer: &[[f64; 2]], surface: u8) {
        let fractions = |c: &[[f64; 2]]| -> Vec<f64> {
            let total = poly_len(c).max(1e-12);
            let mut acc = 0.0;
            let mut f = vec![0.0];
            for w in c.windows(2) {
                acc += len(sub(w[1], w[0]));
                f.push(acc / total);
            }
            f
        };
        let (fi, fo) = (fractions(inner), fractions(outer));
        let (ni, no) = (inner.len(), outer.len());
        let (mut i, mut j) = (0, 0);
        while i < ni - 1 || j < no - 1 {
            let advance_inner = j == no - 1 || (i < ni - 1 && fi[i + 1] <= fo[j + 1]);
            if advance_inner {
                self.tri(inner[i], outer[j], inner[i + 1], surface);
                i += 1;
            } else {
                self.tri(inner[i], outer[j], outer[j + 1], surface);
                j += 1;
            }
        }
    }
}

pub fn build(g: &RoadGraph, r: &ResolvedRoute, opts: &NetworkOptions) -> Result<Network, String> {
    if !r.closed {
        return Err("la red con cruces solo se arma para circuitos cerrados por ahora (usa --closed)".into());
    }
    let index: BTreeMap<&str, usize> = g.edges.iter().enumerate().map(|(i, e)| (e.id.as_str(), i)).collect();
    let route_edges: BTreeSet<usize> =
        r.legs.iter().flat_map(|l| l.steps.iter()).filter_map(|s| index.get(&s.0[1..]).copied()).collect();
    let mut incident: Vec<Vec<usize>> = vec![Vec::new(); g.vertices.len()];
    for (i, e) in g.edges.iter().enumerate() {
        incident[e.from].push(i);
        incident[e.to].push(i);
    }
    let degree = |v: usize| incident[v].len();
    let junction_set: BTreeSet<usize> = route_edges
        .iter()
        .flat_map(|&e| [g.edges[e].from, g.edges[e].to])
        .filter(|&v| degree(v) >= 3)
        .collect();
    if junction_set.is_empty() {
        return Err("el circuito no pasa por ningún cruce".into());
    }

    // 1. Corredores: cadenas de aristas entre cruces, atravesando vértices de grado 2.
    let mut corridors: Vec<Corridor> = Vec::new();
    let mut used: BTreeSet<usize> = BTreeSet::new();
    for &v in &junction_set {
        for &e0 in &incident[v] {
            if used.contains(&e0) {
                continue;
            }
            let on_route = route_edges.contains(&e0);
            let (mut cur_v, mut cur_e) = (v, e0);
            let mut edges = Vec::new();
            let end = loop {
                let e = &g.edges[cur_e];
                let fwd = e.from == cur_v;
                let next = if fwd { e.to } else { e.from };
                edges.push((cur_e, fwd));
                used.insert(cur_e);
                if junction_set.contains(&next) {
                    break Some(next);
                }
                if degree(next) != 2 {
                    break None;
                }
                match incident[next].iter().copied().find(|&x| x != cur_e) {
                    Some(ne) if !used.contains(&ne) && route_edges.contains(&ne) == on_route => {
                        cur_v = next;
                        cur_e = ne;
                    }
                    _ => break None,
                }
            };
            if on_route && end.is_none() {
                return Err(format!("el recorrido termina en una calle sin salida cerca de {}", g.edges[e0].id));
            }
            let mut poly: Vec<[f64; 2]> = Vec::new();
            for &(ei, fwd) in &edges {
                let mut pts = g.edges[ei].en.clone();
                if !fwd {
                    pts.reverse();
                }
                if !poly.is_empty() {
                    pts.remove(0);
                }
                poly.extend(pts);
            }
            let width: f64 = edges.iter().map(|&(ei, _)| g.edges[ei].width_m).sum::<f64>() / edges.len() as f64;
            let fine = resample(&poly, 1.0);
            let axis = if fine.len() > 3 { gaussian(&fine, 1.0, opts.smooth_sigma_m, false) } else { fine };
            corridors.push(Corridor {
                edges,
                start: v,
                end,
                on_route,
                axis,
                half: width.clamp(opts.min_width_m, opts.max_width_m) * 0.5,
            });
        }
    }

    // 2. Brazos de cada cruce.
    let mut arms_at: BTreeMap<usize, Vec<Arm>> = BTreeMap::new();
    for (ci, c) in corridors.iter().enumerate() {
        let l = poly_len(&c.axis);
        let probe = (l / 3.0).min(10.0);
        let d0 = unit(sub(point_at(&c.axis, probe), c.axis[0]));
        arms_at.entry(c.start).or_default().push(Arm { corridor: ci, at_start: true, d: d0, half: c.half });
        if let Some(e) = c.end {
            let d1 = unit(sub(point_at(&c.axis, l - probe), *c.axis.last().unwrap()));
            arms_at.entry(e).or_default().push(Arm { corridor: ci, at_start: false, d: d1, half: c.half });
        }
    }

    // 3. Geometría de cada cruce.
    let mut geoms: BTreeMap<usize, JunctionGeom> = BTreeMap::new();
    for (&v, arms) in arms_at.iter_mut() {
        arms.sort_by(|a, b| angle(a.d).total_cmp(&angle(b.d)));
        let n = arms.len();
        let center = g.vertices[v].en;
        let sw = opts.sidewalk_m;
        let mut setback: Vec<f64> = arms.iter().map(|a| a.half + 1.0).collect();
        let mut sector_deg = Vec::with_capacity(n);
        // Por sector: None = sigue derecho; Some((esquina interior, tangentes, centro del arco, esquina de fachada)).
        let mut corners: Vec<Option<Corner>> = Vec::with_capacity(n);
        for i in 0..n {
            let (a, b) = (arms[i], arms[(i + 1) % n]);
            let mut theta = angle(b.d) - angle(a.d);
            if theta <= 0.0 {
                theta += std::f64::consts::TAU;
            }
            if n == 1 {
                theta = std::f64::consts::TAU;
            }
            sector_deg.push(theta.to_degrees());
            if theta.to_degrees() >= opts.straight_deg {
                corners.push(None);
                continue;
            }
            let pa = add(center, mul(left(a.d), a.half));
            let pb = add(center, mul(left(b.d), -b.half));
            let oa = add(center, mul(left(a.d), a.half + sw));
            let ob = add(center, mul(left(b.d), -(b.half + sw)));
            let (Some(c), Some(co)) = (intersect(pa, a.d, pb, b.d), intersect(oa, a.d, ob, b.d)) else {
                corners.push(None);
                continue;
            };
            let k = opts.curb_radius_m / libm::tan(theta * 0.5);
            let ta = add(c, mul(a.d, k));
            let tb = add(c, mul(b.d, k));
            let bis = unit(add(a.d, b.d));
            let arc_c = add(c, mul(bis, opts.curb_radius_m / libm::sin(theta * 0.5)));
            setback[i] = setback[i].max(dot(sub(ta, center), a.d)).max(dot(sub(co, center), a.d));
            let j = (i + 1) % n;
            setback[j] = setback[j].max(dot(sub(tb, center), b.d)).max(dot(sub(co, center), b.d));
            corners.push(Some((ta, tb, arc_c, co, theta)));
        }
        let mouths: Vec<[[f64; 2]; 4]> = arms
            .iter()
            .zip(&setback)
            .map(|(a, &t)| {
                let m = add(center, mul(a.d, t));
                let l = left(a.d);
                [
                    add(m, mul(l, -(a.half + sw))),
                    add(m, mul(l, -a.half)),
                    add(m, mul(l, a.half)),
                    add(m, mul(l, a.half + sw)),
                ]
            })
            .collect();
        let mut inner = Vec::with_capacity(n);
        let mut outer = Vec::with_capacity(n);
        for i in 0..n {
            let j = (i + 1) % n;
            let (ml, mr) = (mouths[i][2], mouths[j][1]);
            let (ol, or) = (mouths[i][3], mouths[j][0]);
            match corners[i] {
                None => {
                    inner.push(vec![ml, mr]);
                    outer.push(vec![ol, or]);
                }
                Some((ta, tb, arc_c, co, theta)) => {
                    let mut chain = vec![ml];
                    if len(sub(ta, ml)) > 1e-6 {
                        chain.push(ta);
                    }
                    let (fa, fb) = (angle(sub(ta, arc_c)), angle(sub(tb, arc_c)));
                    let mut delta = fb - fa;
                    while delta > std::f64::consts::PI {
                        delta -= std::f64::consts::TAU;
                    }
                    while delta < -std::f64::consts::PI {
                        delta += std::f64::consts::TAU;
                    }
                    debug_assert!((delta.abs() - (std::f64::consts::PI - theta)).abs() < 1e-6);
                    for s in 1..opts.arc_segments {
                        let f = fa + delta * s as f64 / opts.arc_segments as f64;
                        chain.push(add(arc_c, [opts.curb_radius_m * libm::cos(f), opts.curb_radius_m * libm::sin(f)]));
                    }
                    if len(sub(mr, tb)) > 1e-6 {
                        chain.push(tb);
                    }
                    chain.push(mr);
                    inner.push(chain);
                    let mut o = vec![ol];
                    if len(sub(co, ol)) > 1e-6 && len(sub(co, or)) > 1e-6 {
                        o.push(co);
                    }
                    o.push(or);
                    outer.push(o);
                }
            }
        }
        let arc_center = corners.iter().map(|c| c.map(|(_, _, ac, _, _)| ac)).collect();
        geoms.insert(v, JunctionGeom { center, arms: arms.clone(), mouths, inner, outer, setback, sector_deg, arc_center });
    }

    // 4. Estado de cada cruce.
    let mut net = Network::default();
    let mut bad: BTreeMap<usize, String> = BTreeMap::new();
    for (ci, c) in corridors.iter().enumerate() {
        let l = poly_len(&c.axis);
        let t0 = arm_setback(&geoms, c.start, ci, true);
        let t1 = c.end.map_or(0.0, |e| arm_setback(&geoms, e, ci, false));
        if t0 + t1 + 2.0 > l {
            let why = format!("dos cruces a {l:.0} m: sus áreas se superponen");
            bad.insert(c.start, why.clone());
            if let Some(e) = c.end {
                bad.insert(e, why);
            }
        }
    }
    for (&v, jg) in &geoms {
        let n = jg.arms.len();
        let min_angle = jg.sector_deg.iter().copied().fold(f64::INFINITY, f64::min);
        let max_angle = jg.sector_deg.iter().copied().fold(0.0, f64::max);
        let widths: Vec<f64> = jg.arms.iter().map(|a| a.half * 2.0).collect();
        let ratio = widths.iter().copied().fold(0.0, f64::max) / widths.iter().copied().fold(f64::INFINITY, f64::min);
        let layers: BTreeSet<(i32, bool)> = jg
            .arms
            .iter()
            .map(|a| {
                let c = &corridors[a.corridor];
                let (ei, _) = if a.at_start { c.edges[0] } else { *c.edges.last().unwrap() };
                (g.edges[ei].layer, g.edges[ei].bridge || g.edges[ei].tunnel)
            })
            .collect();
        let kind = match n {
            3 if max_angle >= 150.0 => "T".to_string(),
            3 => "Y".to_string(),
            4 => "cruce".to_string(),
            k => format!("{k} brazos"),
        };
        let (status, reason) = if let Some(why) = bad.get(&v) {
            (Status::Unsupported, Some(why.clone()))
        } else if n >= 5 {
            (Status::Unsupported, Some(format!("{n} brazos: todavía no se modelan")))
        } else if layers.len() > 1 {
            (Status::Unsupported, Some("los brazos están a distinto nivel (puente o túnel)".into()))
        } else if min_angle < 30.0 {
            (Status::Unsupported, Some(format!("dos calles se juntan a {min_angle:.0}°")))
        } else if min_angle < 60.0 {
            (Status::Degraded, Some(format!("ángulo cerrado entre calles: {min_angle:.0}°")))
        } else if max_angle > 200.0 {
            (Status::Degraded, Some(format!("todas las calles salen hacia un mismo lado ({max_angle:.0}° libres)")))
        } else if ratio > 1.6 {
            (Status::Degraded, Some(format!("anchos muy distintos entre calles ({ratio:.1}×)")))
        } else {
            (Status::Valid, None)
        };
        let arm_edges = jg
            .arms
            .iter()
            .map(|a| {
                let c = &corridors[a.corridor];
                let (ei, _) = if a.at_start { c.edges[0] } else { *c.edges.last().unwrap() };
                g.edges[ei].id.clone()
            })
            .collect();
        net.junctions.push(Junction { vertex: v, en: jg.center, kind, arms: n, min_angle_deg: min_angle, status, reason, arm_edges });
    }

    // 5. Mallas: cruces (calzada en abanico, veredas en bandas) y corredores.
    let mut mb = MeshBuilder::default();
    for jg in geoms.values() {
        let n = jg.arms.len();
        let mut ring: Vec<[f64; 2]> = Vec::new();
        for i in 0..n {
            ring.push(jg.mouths[i][1]);
            ring.push(jg.mouths[i][2]);
            let chain = &jg.inner[i];
            ring.extend_from_slice(&chain[1..chain.len() - 1]);
        }
        for k in 0..ring.len() {
            mb.tri(jg.center, ring[k], ring[(k + 1) % ring.len()], ASPHALT);
        }
        for i in 0..n {
            mb.band(&jg.inner[i], &jg.outer[i], SIDEWALK);
            net.walls.push(jg.outer[i].clone());
        }
    }
    for (ci, c) in corridors.iter().enumerate() {
        let l = poly_len(&c.axis);
        let t0 = arm_setback(&geoms, c.start, ci, true);
        let t1 = match c.end {
            Some(e) => l - arm_setback(&geoms, e, ci, false),
            None => (t0 + opts.stub_m).min(l - 6.0).max(t0 + 8.0).min(l),
        };
        if t1 - t0 < 0.5 {
            continue; // cruces superpuestos: ya marcados como no soportados
        }
        net.street_length_m += t1 - t0;
        let piece = slice(&c.axis, t0, t1);
        let pts = resample(&piece, opts.spacing_m);
        let m = pts.len();
        let mut sections: Vec<[[f64; 2]; 4]> = (0..m)
            .map(|i| {
                let t = unit(sub(pts[(i + 1).min(m - 1)], pts[i.saturating_sub(1)]));
                let l = left(t);
                let h = c.half;
                let s = opts.sidewalk_m;
                [add(pts[i], mul(l, -(h + s))), add(pts[i], mul(l, -h)), add(pts[i], mul(l, h)), add(pts[i], mul(l, h + s))]
            })
            .collect();
        sections[0] = mouth(&geoms, c.start, ci, true);
        if let Some(e) = c.end {
            let mo = mouth(&geoms, e, ci, false);
            sections[m - 1] = [mo[3], mo[2], mo[1], mo[0]];
        }
        for w in sections.windows(2) {
            let (a, b) = (w[0], w[1]);
            mb.quad(a[0], b[0], b[1], a[1], SIDEWALK);
            mb.quad(a[1], b[1], b[2], a[2], ASPHALT);
            mb.quad(a[2], b[2], b[3], a[3], SIDEWALK);
        }
        net.walls.push(sections.iter().map(|s| s[0]).collect());
        net.walls.push(sections.iter().rev().map(|s| s[3]).collect());
        if c.end.is_none() {
            let last = sections[m - 1];
            net.walls.push(vec![last[0], last[3]]);
        }
    }
    net.vertices = mb.vertices;
    net.triangles = mb.triangles;
    net.reference = reference(g, r, &corridors, &geoms, &index, opts)?;

    // 6. Diagnósticos.
    let count = |s: Status| net.junctions.iter().filter(|j| j.status == s).count();
    net.diagnostics.push(Diagnostic::new(
        Severity::Info,
        "junctions",
        format!(
            "{} cruces modelados: {} válidos, {} degradados, {} no soportados",
            net.junctions.len(),
            count(Status::Valid),
            count(Status::Degraded),
            count(Status::Unsupported)
        ),
    ));
    let side = corridors.iter().filter(|c| !c.on_route).count();
    if side > 0 {
        net.diagnostics.push(Diagnostic::new(
            Severity::Info,
            "side-streets",
            format!("{side} calles laterales conducibles; las que no llevan a otro cruce del circuito se cierran a {:.0} m", opts.stub_m),
        ));
    }
    Ok(net)
}

/// Línea de referencia: los corredores del recorrido en orden, unidos en cada cruce por un
/// tramo recto (si sigue derecho) o por un arco concéntrico a la esquina (si dobla).
fn reference(
    g: &RoadGraph,
    r: &ResolvedRoute,
    corridors: &[Corridor],
    geoms: &BTreeMap<usize, JunctionGeom>,
    index: &BTreeMap<&str, usize>,
    opts: &NetworkOptions,
) -> Result<Vec<RefPoint>, String> {
    // Corredor y sentido de cada paso del recorrido, sin repetir consecutivos.
    let mut of_edge: BTreeMap<usize, (usize, bool)> = BTreeMap::new();
    for (ci, c) in corridors.iter().enumerate() {
        for &(ei, fwd) in &c.edges {
            of_edge.insert(ei, (ci, fwd));
        }
    }
    let mut seq: Vec<(usize, bool)> = Vec::new();
    for st in r.legs.iter().flat_map(|l| l.steps.iter()) {
        let Some(&ei) = index.get(&st.0[1..]) else { continue };
        let (ci, cfwd) = of_edge[&ei];
        let forward = st.0.starts_with('+') == cfwd;
        if seq.last().map(|x| x.0) != Some(ci) {
            seq.push((ci, forward));
        }
    }
    while seq.len() > 1 && seq.first().map(|x| x.0) == seq.last().map(|x| x.0) {
        seq.pop();
    }
    if seq.len() < 2 {
        return Err("el circuito necesita pasar por al menos dos calles".into());
    }
    let n = seq.len();
    let travel = |k: usize| -> Vec<[f64; 2]> {
        let (ci, fwd) = seq[k];
        let mut a = corridors[ci].axis.clone();
        if !fwd {
            a.reverse();
        }
        a
    };
    // Para cada cruce entre el corredor k y el k+1: recorte al final de k, recorte al inicio
    // de k+1 y los puntos del arco.
    let mut trim_end = vec![0.0; n];
    let mut trim_start = vec![0.0; n];
    let mut arcs: Vec<Vec<[f64; 2]>> = vec![Vec::new(); n];
    for k in 0..n {
        let j = (k + 1) % n;
        let (ci, fi) = seq[k];
        let (co, fo) = seq[j];
        let v = if fi { corridors[ci].end } else { Some(corridors[ci].start) }
            .ok_or("el recorrido sale por una calle lateral")?;
        let jg = &geoms[&v];
        let arm = |c: usize, at_start: bool| jg.arms.iter().position(|a| a.corridor == c && a.at_start == at_start).unwrap();
        let (ia, ib) = (arm(ci, !fi), arm(co, fo));
        let m = jg.arms.len();
        // Sector entre los dos brazos, si son vecinos.
        let sector = if (ia + 1) % m == ib {
            Some(ia)
        } else if (ib + 1) % m == ia {
            Some(ib)
        } else {
            None
        };
        let Some(c) = sector.and_then(|s| jg.arc_center[s]) else { continue };
        let (din, dout) = (jg.arms[ia].d, jg.arms[ib].d);
        let (tin, tout) = (dot(sub(c, jg.center), din), dot(sub(c, jg.center), dout));
        let pin = add(jg.center, mul(din, tin));
        let pout = add(jg.center, mul(dout, tout));
        let (rin, rout) = (len(sub(pin, c)), len(sub(pout, c)));
        let (fa, fb) = (angle(sub(pin, c)), angle(sub(pout, c)));
        let mut delta = fb - fa;
        while delta > std::f64::consts::PI {
            delta -= std::f64::consts::TAU;
        }
        while delta < -std::f64::consts::PI {
            delta += std::f64::consts::TAU;
        }
        let steps = ((delta.abs() * (rin + rout) * 0.5).ceil() as usize).max(2);
        arcs[k] = (0..=steps)
            .map(|q| {
                let f = q as f64 / steps as f64;
                let rad = rin + (rout - rin) * f;
                let a = fa + delta * f;
                add(c, [rad * libm::cos(a), rad * libm::sin(a)])
            })
            .collect();
        trim_end[k] = tin;
        trim_start[j] = tout;
    }
    let mut out: Vec<RefPoint> = Vec::new();
    for k in 0..n {
        let (ci, _) = seq[k];
        let c = &corridors[ci];
        let axis = travel(k);
        let l = poly_len(&axis);
        let (a, b) = (trim_start[k], l - trim_end[k]);
        if b - a < 1.0 {
            return Err(format!("el tramo de {} entre dos giros es demasiado corto", g.edges[c.edges[0].0].id));
        }
        let edge = c.edges[0].0;
        let piece = slice(&axis, a, b);
        // Si el siguiente cruce es recto, el eje llega hasta el centro y el siguiente sale de
        // ahí: se omite el punto repetido.
        for p in resample(&piece, 1.0) {
            if out.last().is_none_or(|q| len(sub(q.en, p)) > 0.05) {
                out.push(RefPoint { en: p, width: c.half * 2.0, edge });
            }
        }
        for &p in arcs[k].iter().skip(1) {
            out.push(RefPoint { en: p, width: c.half * 2.0, edge });
        }
    }
    if out.last().is_some_and(|q| len(sub(q.en, out[0].en)) < 0.05) {
        out.pop();
    }
    // Empieza junto al primer punto del recorrido.
    let start = r.en[0];
    let k0 = (0..out.len()).min_by(|&x, &y| len(sub(out[x].en, start)).total_cmp(&len(sub(out[y].en, start)))).unwrap();
    out.rotate_left(k0);
    // Paso uniforme de 1 m sobre el lazo cerrado, con un suavizado corto para los empalmes.
    let mut ring: Vec<[f64; 2]> = out.iter().map(|q| q.en).collect();
    ring.push(ring[0]);
    let even = resample(&ring, 1.0);
    let even = &even[..even.len() - 1];
    let smooth = gaussian(even, 1.0, opts.smooth_sigma_m * 0.5, true);
    let mut cursor = 0usize;
    Ok(smooth
        .into_iter()
        .map(|p| {
            // Atributos del punto original más cercano, avanzando en orden.
            let mut best = cursor;
            for q in cursor..(cursor + 8).min(out.len()) {
                if len(sub(out[q].en, p)) < len(sub(out[best].en, p)) {
                    best = q;
                }
            }
            cursor = best;
            RefPoint { en: p, width: out[best].width, edge: out[best].edge }
        })
        .collect())
}

fn arm_of(geoms: &BTreeMap<usize, JunctionGeom>, v: usize, corridor: usize, at_start: bool) -> (&JunctionGeom, usize) {
    let jg = &geoms[&v];
    let k = jg.arms.iter().position(|a| a.corridor == corridor && a.at_start == at_start).unwrap();
    (jg, k)
}

fn arm_setback(geoms: &BTreeMap<usize, JunctionGeom>, v: usize, corridor: usize, at_start: bool) -> f64 {
    let (jg, k) = arm_of(geoms, v, corridor, at_start);
    jg.setback[k]
}

fn mouth(geoms: &BTreeMap<usize, JunctionGeom>, v: usize, corridor: usize, at_start: bool) -> [[f64; 2]; 4] {
    let (jg, k) = arm_of(geoms, v, corridor, at_start);
    jg.mouths[k]
}

impl Junction {
    pub fn describe(&self, frame: &LocalFrame) -> String {
        let p = frame.en_to_geo(self.en);
        format!("{} en {:.6},{:.6}", self.kind, p.lat, p.lon)
    }
}
