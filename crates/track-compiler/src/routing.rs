//! Ruteo entre puntos de paso sobre el grafo.
//!
//! - Cada punto se ajusta a la arista más cercana (proyección sobre el segmento).
//! - A* con heurística de distancia en línea recta (admisible: ninguna arista es más corta
//!   que la recta entre sus extremos).
//! - Desempates por índice de vértice: misma entrada, mismo recorrido, en cualquier máquina.
//! - En carreras se ignoran los sentidos por defecto (pregunta abierta del documento);
//!   `respect_oneway` los activa.
//! - La búsqueda recorre aristas dirigidas, no vértices: así sabe por dónde llega a cada
//!   cruce y puede limitar el ángulo de giro. Sin esto, la ruta más corta entra a una
//!   rampa con una media vuelta de 170° que en una pista sería imposible de manejar.

use crate::geo::GeoPoint;
use crate::graph::RoadGraph;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::BinaryHeap;

/// Distancia máxima para ajustar un punto a una calle.
pub const MAX_SNAP_M: f64 = 60.0;

#[derive(Clone, Copy, Debug)]
pub struct RouteOptions {
    pub respect_oneway: bool,
    /// Agrega el tramo del último punto al primero.
    pub closed: bool,
    /// Giro máximo permitido en un cruce, en grados (0 = seguir derecho, 180 = media vuelta).
    pub max_turn_deg: f64,
    /// Costo extra de un giro de 90°, en metros equivalentes. Crece con el cuadrado del ángulo.
    pub turn_cost_m: f64,
}

impl Default for RouteOptions {
    fn default() -> Self {
        Self { respect_oneway: false, closed: false, max_turn_deg: 135.0, turn_cost_m: 20.0 }
    }
}

impl RouteOptions {
    /// Sin límite de giro ni costo: la ruta más corta pura (solo para comparar).
    pub fn shortest() -> Self {
        Self { max_turn_deg: 180.0, turn_cost_m: 0.0, ..Self::default() }
    }

    /// Costo del giro entre dos direcciones, o `None` si supera el máximo.
    fn turn(&self, a: [f64; 2], b: [f64; 2]) -> Option<f64> {
        let deg = turn_angle_deg(a, b);
        if deg > self.max_turn_deg + 1e-9 {
            return None;
        }
        let k = deg / 90.0;
        Some(self.turn_cost_m * k * k)
    }
}

/// Ángulo entre dos direcciones de marcha, en grados (0 = derecho, 180 = media vuelta).
pub fn turn_angle_deg(a: [f64; 2], b: [f64; 2]) -> f64 {
    let cross = a[0] * b[1] - a[1] * b[0];
    let dot = a[0] * b[0] + a[1] * b[1];
    libm::atan2(cross, dot).abs().to_degrees()
}


#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Snap {
    pub edge: String,
    /// Distancia desde el inicio de la arista hasta el punto ajustado.
    pub along_m: f64,
    /// Distancia del punto pedido a la calle.
    pub offset_m: f64,
    pub point: GeoPoint,
    #[serde(skip)]
    pub edge_index: usize,
    #[serde(skip)]
    pub en: [f64; 2],
}

/// Arista recorrida: `+w123#0` en el sentido de su geometría, `-w123#0` al revés.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Step(pub String);

impl Step {
    fn new(id: &str, forward: bool) -> Self {
        Step(format!("{}{}", if forward { '+' } else { '-' }, id))
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Leg {
    pub from: Snap,
    pub to: Snap,
    pub steps: Vec<Step>,
    pub length_m: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedRoute {
    pub graph_region: String,
    pub graph_hash: String,
    pub respect_oneway: bool,
    pub closed: bool,
    pub max_turn_deg: f64,
    pub turn_cost_m: f64,
    pub waypoints: Vec<GeoPoint>,
    pub legs: Vec<Leg>,
    pub length_m: f64,
    /// Geometría completa del recorrido, en el plano local.
    #[serde(skip)]
    pub en: Vec<[f64; 2]>,
    /// Geometría completa en grados.
    pub geo: Vec<GeoPoint>,
}

fn dist(a: [f64; 2], b: [f64; 2]) -> f64 {
    libm::hypot(b[0] - a[0], b[1] - a[1])
}

/// Proyección de `p` sobre el segmento a-b: (parámetro t, punto).
fn project(p: [f64; 2], a: [f64; 2], b: [f64; 2]) -> (f64, [f64; 2]) {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let l2 = dx * dx + dy * dy;
    let t = if l2 < 1e-12 { 0.0 } else { (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / l2).clamp(0.0, 1.0) };
    (t, [a[0] + dx * t, a[1] + dy * t])
}

/// Distancia del punto a la calle más cercana, en metros.
fn nearest_distance(g: &RoadGraph, p: GeoPoint) -> f64 {
    let q = g.frame.to_en(p);
    let mut best = f64::INFINITY;
    for e in &g.edges {
        for w in e.en.windows(2) {
            best = best.min(dist(q, project(q, w[0], w[1]).1));
        }
    }
    best
}

/// Mejor tramo encontrado entre dos puntos ajustados: (costo, pasos intermedios, directo).
type Candidate = (f64, Vec<(usize, bool)>, bool);

pub fn snap(g: &RoadGraph, p: GeoPoint) -> Result<Snap, String> {
    let q = g.frame.to_en(p);
    let mut best: Option<(f64, usize, f64, [f64; 2])> = None; // (distancia, arista, along, punto)
    for (ei, e) in g.edges.iter().enumerate() {
        let mut acc = 0.0;
        for w in e.en.windows(2) {
            let (t, pt) = project(q, w[0], w[1]);
            let d = dist(q, pt);
            let seg = dist(w[0], w[1]);
            if best.is_none_or(|b| d < b.0) {
                best = Some((d, ei, acc + seg * t, pt));
            }
            acc += seg;
        }
    }
    let (d, ei, along, pt) = best.ok_or("el grafo no tiene aristas")?;
    if d > MAX_SNAP_M {
        // Si con lat y lon intercambiadas el punto sí cae en la región, casi seguro es eso.
        let swapped = GeoPoint::new(p.lon, p.lat);
        let hint = if (-90.0..=90.0).contains(&p.lon) && nearest_distance(g, swapped) <= MAX_SNAP_M {
            format!(" ¿Está invertido el orden? Como lat,lon sería {:.5},{:.5}, que sí está en la región.", swapped.lat, swapped.lon)
        } else {
            String::new()
        };
        return Err(format!(
            "el punto {:.5},{:.5} está a {:.0} m de la calle más cercana (máximo {MAX_SNAP_M:.0} m).{hint}",
            p.lat, p.lon, d
        ));
    }
    Ok(Snap { edge: g.edges[ei].id.clone(), along_m: along, offset_m: d, point: g.frame.en_to_geo(pt), edge_index: ei, en: pt })
}

#[derive(PartialEq)]
struct Open {
    f: f64,
    v: usize,
}
impl Eq for Open {}
impl Ord for Open {
    fn cmp(&self, o: &Self) -> Ordering {
        // BinaryHeap es de máximos: se invierte. Desempate por índice de vértice.
        o.f.total_cmp(&self.f).then_with(|| o.v.cmp(&self.v))
    }
}
impl PartialOrd for Open {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}

/// Recorte de la geometría de una arista entre dos distancias `a` ≤ `b` desde su inicio.
fn cut(en: &[[f64; 2]], a: f64, b: f64) -> Vec<[f64; 2]> {
    let mut out = Vec::new();
    let mut acc = 0.0;
    let lerp = |p: [f64; 2], q: [f64; 2], t: f64| [p[0] + (q[0] - p[0]) * t, p[1] + (q[1] - p[1]) * t];
    for w in en.windows(2) {
        let seg = dist(w[0], w[1]);
        let (s0, s1) = (acc, acc + seg);
        if s1 >= a && s0 <= b && seg > 0.0 {
            let t0 = ((a - s0) / seg).clamp(0.0, 1.0);
            let t1 = ((b - s0) / seg).clamp(0.0, 1.0);
            let p0 = lerp(w[0], w[1], t0);
            if out.last() != Some(&p0) {
                out.push(p0);
            }
            out.push(lerp(w[0], w[1], t1));
        }
        acc = s1;
    }
    out
}

/// Dirección de marcha al llegar al final de una arista recorrida en sentido `fwd`.
fn dir_arriving(e: &crate::graph::Edge, fwd: bool) -> [f64; 2] {
    let n = e.en.len();
    let (p, q) = if fwd { (e.en[n - 2], e.en[n - 1]) } else { (e.en[1], e.en[0]) };
    [q[0] - p[0], q[1] - p[1]]
}

/// Dirección de marcha al salir del inicio de una arista recorrida en sentido `fwd`.
fn dir_leaving(e: &crate::graph::Edge, fwd: bool) -> [f64; 2] {
    let n = e.en.len();
    let (p, q) = if fwd { (e.en[0], e.en[1]) } else { (e.en[n - 1], e.en[n - 2]) };
    [q[0] - p[0], q[1] - p[1]]
}

/// Dirección de marcha sobre una arista en el punto a `along` metros de su inicio.
fn tangent_at(e: &crate::graph::Edge, along: f64, fwd: bool) -> [f64; 2] {
    let mut acc = 0.0;
    let mut d = [e.en[1][0] - e.en[0][0], e.en[1][1] - e.en[0][1]];
    for w in e.en.windows(2) {
        let seg = dist(w[0], w[1]);
        d = [w[1][0] - w[0][0], w[1][1] - w[0][1]];
        if acc + seg >= along {
            break;
        }
        acc += seg;
    }
    if fwd { d } else { [-d[0], -d[1]] }
}

/// Un punto a menos de esto de un vértice cuenta como "en el cruce": se puede salir o
/// llegar por cualquier calle, sin heredar la dirección de la arista a la que se ajustó.
const AT_VERTEX_M: f64 = 1.0;

/// Estado de la búsqueda: arista dirigida (índice de arista, avance).
fn state(ei: usize, fwd: bool) -> usize {
    ei * 2 + fwd as usize
}

/// `arrive`: dirección de marcha con la que terminó el tramo anterior. Al salir de un punto
/// de paso intermedio vale el mismo límite de giro que en un cruce (no hay media vuelta).
fn leg(
    g: &RoadGraph,
    adj: &[Vec<(usize, bool)>],
    a: &Snap,
    b: &Snap,
    opts: RouteOptions,
    arrive: Option<[f64; 2]>,
) -> Result<(Leg, Vec<[f64; 2]>), String> {
    let ea = &g.edges[a.edge_index];
    let eb = &g.edges[b.edge_index];
    let allow = |e: &crate::graph::Edge, fwd: bool| !opts.respect_oneway || e.oneway.allows(fwd);
    // Costo de salir del punto de paso en una dirección dada (None = prohibido).
    let leave = |dir: [f64; 2]| -> Option<f64> { arrive.map_or(Some(0.0), |a| opts.turn(a, dir)) };

    // Mismo tramo: recorrido directo si el sentido lo permite.
    let mut best: Option<Candidate> = None;
    if a.edge_index == b.edge_index {
        let fwd = b.along_m >= a.along_m;
        if allow(ea, fwd) {
            if let Some(tc) = leave(tangent_at(ea, a.along_m, fwd)) {
                best = Some(((b.along_m - a.along_m).abs() + tc, Vec::new(), true));
            }
        }
    }

    // A* sobre aristas dirigidas. `gcost` incluye el costo de los giros; `glen`, solo metros.
    let ns = g.edges.len() * 2;
    let mut gcost = vec![f64::INFINITY; ns];
    let mut glen = vec![0.0f64; ns];
    let mut prev: Vec<Option<usize>> = vec![None; ns];
    let goal = b.en;
    let end_vertex = |s: usize| {
        let e = &g.edges[s / 2];
        if s % 2 == 1 { e.to } else { e.from }
    };
    let h = |s: usize| dist(g.vertices[end_vertex(s)].en, goal);
    let mut open = BinaryHeap::new();
    // Estados de partida "en el cruce": su dirección de llegada no es la de la arista sino
    // la del tramo anterior (o libre, si es el primer punto).
    let mut free_start: Vec<(usize, Option<[f64; 2]>)> = Vec::new();
    for fwd in [true, false] {
        if !allow(ea, fwd) {
            continue;
        }
        let st = state(a.edge_index, fwd);
        let c = if fwd { ea.length_m - a.along_m } else { a.along_m };
        let tc = if c < AT_VERTEX_M {
            free_start.push((st, arrive));
            0.0
        } else {
            match leave(tangent_at(ea, a.along_m, fwd)) {
                Some(tc) => tc,
                None => continue,
            }
        };
        gcost[st] = c + tc;
        glen[st] = c;
        open.push(Open { f: gcost[st] + h(st), v: st });
    }
    // Llegada: desde `from` de la arista destino avanzando, o desde `to` retrocediendo.
    let mut best_end: Option<(f64, f64, usize, bool)> = None; // (costo, metros, estado, sentido final)
    while let Some(Open { f, v: st }) = open.pop() {
        let limit = best.as_ref().map(|b| b.0).unwrap_or(f64::INFINITY).min(best_end.map_or(f64::INFINITY, |x| x.0));
        if f >= limit {
            break;
        }
        if f > gcost[st] + h(st) + 1e-9 {
            continue; // entrada obsoleta
        }
        let (ei, fwd) = (st / 2, st % 2 == 1);
        let e = &g.edges[ei];
        let v = end_vertex(st);
        let incoming = match free_start.iter().find(|(s0, _)| *s0 == st && prev[st].is_none()) {
            Some((_, dir)) => *dir,
            None => Some(dir_arriving(e, fwd)),
        };
        let turn = |out: [f64; 2]| incoming.map_or(Some(0.0), |i| opts.turn(i, out));
        for end_fwd in [true, false] {
            let at = if end_fwd { eb.from } else { eb.to };
            if v != at || !allow(eb, end_fwd) {
                continue;
            }
            let tail = if end_fwd { b.along_m } else { eb.length_m - b.along_m };
            // Si el destino está en el cruce mismo, se llega por cualquier calle.
            let tc = if tail < AT_VERTEX_M { Some(0.0) } else { turn(dir_leaving(eb, end_fwd)) };
            if let Some(tc) = tc {
                let total = gcost[st] + tc + tail;
                if best_end.is_none_or(|x| total < x.0) {
                    best_end = Some((total, glen[st] + tail, st, end_fwd));
                }
            }
        }
        for &(ej, fwd2) in &adj[v] {
            let e2 = &g.edges[ej];
            let Some(tc) = turn(dir_leaving(e2, fwd2)) else { continue };
            let ns2 = state(ej, fwd2);
            let c = gcost[st] + tc + e2.length_m;
            if c < gcost[ns2] - 1e-9 {
                gcost[ns2] = c;
                glen[ns2] = glen[st] + e2.length_m;
                prev[ns2] = Some(st);
                open.push(Open { f: c + h(ns2), v: ns2 });
            }
        }
    }

    // (metros, aristas intermedias, sentido de salida, sentido de llegada)
    let via_graph = best_end.map(|(c, meters, st, end_fwd)| {
        let mut chain = Vec::new();
        let mut cur = st;
        while let Some(p) = prev[cur] {
            chain.push((cur / 2, cur % 2 == 1));
            cur = p;
        }
        chain.reverse();
        (c, meters, chain, cur % 2 == 1, end_fwd)
    });

    let use_direct = match (&best, &via_graph) {
        (Some(d), Some(v)) => d.0 <= v.0,
        (Some(_), None) => true,
        (None, Some(_)) => false,
        (None, None) => {
            return Err(format!(
                "no hay ruta entre {:.5},{:.5} y {:.5},{:.5}{} sin giros de más de {:.0}°",
                a.point.lat,
                a.point.lon,
                b.point.lat,
                b.point.lon,
                if opts.respect_oneway { " respetando los sentidos de las calles" } else { "" },
                opts.max_turn_deg
            ))
        }
    };

    let mut steps = Vec::new();
    let mut en: Vec<[f64; 2]> = Vec::new();
    let push = |en: &mut Vec<[f64; 2]>, pts: Vec<[f64; 2]>| {
        for p in pts {
            if en.last().is_none_or(|q| dist(*q, p) > 1e-6) {
                en.push(p);
            }
        }
    };
    let length;
    if use_direct {
        let (c, _, _) = best.unwrap();
        length = c;
        let fwd = b.along_m >= a.along_m;
        steps.push(Step::new(&ea.id, fwd));
        let (lo, hi) = (a.along_m.min(b.along_m), a.along_m.max(b.along_m));
        let mut pts = cut(&ea.en, lo, hi);
        if !fwd {
            pts.reverse();
        }
        push(&mut en, pts);
    } else {
        let (_, meters, chain, start_fwd, end_fwd) = via_graph.unwrap();
        length = meters;
        // Primer tramo parcial (se omite si el punto está en el cruce mismo).
        let head = if start_fwd { ea.length_m - a.along_m } else { a.along_m };
        if head >= AT_VERTEX_M {
            steps.push(Step::new(&ea.id, start_fwd));
        }
        let mut pts = if start_fwd { cut(&ea.en, a.along_m, ea.length_m) } else { cut(&ea.en, 0.0, a.along_m) };
        if !start_fwd {
            pts.reverse();
        }
        push(&mut en, pts);
        for (ei, fwd) in &chain {
            let e = &g.edges[*ei];
            steps.push(Step::new(&e.id, *fwd));
            let mut pts = e.en.clone();
            if !*fwd {
                pts.reverse();
            }
            push(&mut en, pts);
        }
        // Último tramo parcial (ídem).
        let tail = if end_fwd { b.along_m } else { eb.length_m - b.along_m };
        if tail >= AT_VERTEX_M || steps.is_empty() {
            steps.push(Step::new(&eb.id, end_fwd));
        }
        let mut pts = if end_fwd { cut(&eb.en, 0.0, b.along_m) } else { cut(&eb.en, b.along_m, eb.length_m) };
        if !end_fwd {
            pts.reverse();
        }
        push(&mut en, pts);
    }
    Ok((Leg { from: a.clone(), to: b.clone(), steps, length_m: length }, en))
}

pub fn route(g: &RoadGraph, waypoints: &[GeoPoint], opts: RouteOptions) -> Result<ResolvedRoute, String> {
    if waypoints.len() < 2 {
        return Err("se necesitan al menos dos puntos".into());
    }
    let snaps: Vec<Snap> = waypoints.iter().map(|p| snap(g, *p)).collect::<Result<_, _>>()?;
    for w in snaps.windows(2) {
        let (ca, cb) = (g.component[g.edges[w[0].edge_index].from], g.component[g.edges[w[1].edge_index].from]);
        if ca != cb {
            return Err(format!(
                "los puntos {:.5},{:.5} y {:.5},{:.5} están en partes del mapa que no se conectan",
                w[0].point.lat, w[0].point.lon, w[1].point.lat, w[1].point.lon
            ));
        }
    }
    let adj = g.adjacency(opts.respect_oneway);
    let mut pairs: Vec<(usize, usize)> = (0..snaps.len() - 1).map(|i| (i, i + 1)).collect();
    if opts.closed {
        pairs.push((snaps.len() - 1, 0));
    }
    let mut legs = Vec::new();
    let mut en: Vec<[f64; 2]> = Vec::new();
    let mut arrive: Option<[f64; 2]> = None;
    for (i, j) in pairs {
        let (l, pts) = leg(g, &adj, &snaps[i], &snaps[j], opts, arrive)?;
        if let [.., p, q] = pts[..] {
            arrive = Some([q[0] - p[0], q[1] - p[1]]);
        }
        for p in pts {
            if en.last().is_none_or(|q| dist(*q, p) > 1e-6) {
                en.push(p);
            }
        }
        legs.push(l);
    }
    let length_m = legs.iter().map(|l| l.length_m).sum();
    let geo = en.iter().map(|p| g.frame.en_to_geo(*p)).collect();
    Ok(ResolvedRoute {
        graph_region: g.region.clone(),
        graph_hash: g.hash.clone(),
        respect_oneway: opts.respect_oneway,
        closed: opts.closed,
        max_turn_deg: opts.max_turn_deg,
        turn_cost_m: opts.turn_cost_m,
        waypoints: waypoints.to_vec(),
        legs,
        length_m,
        en,
        geo,
    })
}

impl ResolvedRoute {
    /// Ángulo de giro en cada cruce del recorrido (entre aristas consecutivas), en grados.
    pub fn junction_turns(&self, g: &RoadGraph) -> Vec<f64> {
        let index: std::collections::BTreeMap<&str, usize> =
            g.edges.iter().enumerate().map(|(i, e)| (e.id.as_str(), i)).collect();
        let steps: Vec<(usize, bool)> = self
            .legs
            .iter()
            .flat_map(|l| l.steps.iter())
            .filter_map(|s| {
                let (sign, id) = s.0.split_at(1);
                index.get(id).map(|&i| (i, sign == "+"))
            })
            .collect();
        steps
            .windows(2)
            .filter(|w| w[0] != w[1]) // la misma arista al pasar por un punto de paso
            .map(|w| turn_angle_deg(dir_arriving(&g.edges[w[0].0], w[0].1), dir_leaving(&g.edges[w[1].0], w[1].1)))
            .collect()
    }

    /// Fragmento `route` del descriptor de pista (documento de arquitectura v0.2).
    pub fn descriptor_route(&self) -> serde_json::Value {
        let edges: Vec<&str> = self.legs.iter().flat_map(|l| l.steps.iter().map(|s| s.0.as_str())).collect();
        serde_json::json!({
            "waypoints": self.waypoints,
            "resolvedPath": {
                "graph": format!("{}@{}", self.graph_region, self.graph_hash),
                "respectOneway": self.respect_oneway,
                "maxTurnDeg": self.max_turn_deg,
                "turnCostM": self.turn_cost_m,
                "edges": edges,
            },
            "closed": self.closed,
            "lengthM": (self.length_m * 10.0).round() / 10.0,
        })
    }
}
