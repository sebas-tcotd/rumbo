//! Ruteo entre puntos de paso sobre el grafo.
//!
//! - Cada punto se ajusta a la arista más cercana (proyección sobre el segmento).
//! - A* con heurística de distancia en línea recta (admisible: ninguna arista es más corta
//!   que la recta entre sus extremos).
//! - Desempates por índice de vértice: misma entrada, mismo recorrido, en cualquier máquina.
//! - En carreras se ignoran los sentidos por defecto (pregunta abierta del documento);
//!   `respect_oneway` los activa.

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
}

impl Default for RouteOptions {
    fn default() -> Self {
        Self { respect_oneway: false, closed: false }
    }
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

fn leg(g: &RoadGraph, adj: &[Vec<(usize, bool)>], a: &Snap, b: &Snap, opts: RouteOptions) -> Result<(Leg, Vec<[f64; 2]>), String> {
    let ea = &g.edges[a.edge_index];
    let eb = &g.edges[b.edge_index];
    let allow = |e: &crate::graph::Edge, fwd: bool| !opts.respect_oneway || e.oneway.allows(fwd);

    // Mismo tramo: recorrido directo si el sentido lo permite.
    let mut best: Option<(f64, Vec<(usize, bool)>, bool)> = None; // (costo, pasos intermedios, directo)
    if a.edge_index == b.edge_index {
        let fwd = b.along_m >= a.along_m;
        if allow(ea, fwd) {
            best = Some(((b.along_m - a.along_m).abs(), Vec::new(), true));
        }
    }

    // A* desde los extremos de la arista de partida.
    let n = g.vertices.len();
    let mut gcost = vec![f64::INFINITY; n];
    let mut prev: Vec<Option<(usize, usize, bool)>> = vec![None; n]; // (vértice previo, arista, avance)
    let mut start_dir: Vec<Option<bool>> = vec![None; n];
    let goal = b.en;
    let h = |v: usize| dist(g.vertices[v].en, goal);
    let mut open = BinaryHeap::new();
    let mut seed = |v: usize, c: f64, fwd: bool, gcost: &mut Vec<f64>, open: &mut BinaryHeap<Open>| {
        if c < gcost[v] {
            gcost[v] = c;
            start_dir[v] = Some(fwd);
            open.push(Open { f: c + h(v), v });
        }
    };
    if allow(ea, true) {
        seed(ea.to, ea.length_m - a.along_m, true, &mut gcost, &mut open);
    }
    if allow(ea, false) {
        seed(ea.from, a.along_m, false, &mut gcost, &mut open);
    }
    // Llegada: desde `from` de la arista destino avanzando, o desde `to` retrocediendo.
    let goal_cost = |v: usize| -> Option<(f64, bool)> {
        let mut r: Option<(f64, bool)> = None;
        if v == eb.from && allow(eb, true) {
            r = Some((b.along_m, true));
        }
        if v == eb.to && allow(eb, false) {
            let c = eb.length_m - b.along_m;
            if r.is_none_or(|x| c < x.0) {
                r = Some((c, false));
            }
        }
        r
    };
    let mut best_end: Option<(f64, usize, bool)> = None;
    while let Some(Open { f, v }) = open.pop() {
        let limit = best.as_ref().map(|b| b.0).unwrap_or(f64::INFINITY).min(best_end.map_or(f64::INFINITY, |x| x.0));
        if f >= limit {
            break;
        }
        if f > gcost[v] + h(v) + 1e-9 {
            continue; // entrada obsoleta
        }
        if let Some((c, fwd)) = goal_cost(v) {
            let total = gcost[v] + c;
            if best_end.is_none_or(|x| total < x.0) {
                best_end = Some((total, v, fwd));
            }
        }
        for &(ei, fwd) in &adj[v] {
            let e = &g.edges[ei];
            let to = if fwd { e.to } else { e.from };
            let c = gcost[v] + e.length_m;
            if c < gcost[to] - 1e-9 {
                gcost[to] = c;
                prev[to] = Some((v, ei, fwd));
                open.push(Open { f: c + h(to), v: to });
            }
        }
    }

    let via_graph = best_end.map(|(c, v, end_fwd)| {
        let mut chain = Vec::new();
        let mut cur = v;
        while let Some((p, ei, fwd)) = prev[cur] {
            chain.push((ei, fwd));
            cur = p;
        }
        chain.reverse();
        (c, chain, cur, end_fwd)
    });

    let use_direct = match (&best, &via_graph) {
        (Some(d), Some(v)) => d.0 <= v.0,
        (Some(_), None) => true,
        (None, Some(_)) => false,
        (None, None) => {
            return Err(format!(
                "no hay ruta entre {:.5},{:.5} y {:.5},{:.5}{}",
                a.point.lat,
                a.point.lon,
                b.point.lat,
                b.point.lon,
                if opts.respect_oneway { " respetando los sentidos de las calles" } else { "" }
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
        let (c, chain, first_v, end_fwd) = via_graph.unwrap();
        length = c;
        // Primer tramo parcial.
        let start_fwd = start_dir[first_v].unwrap_or(true);
        steps.push(Step::new(&ea.id, start_fwd));
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
        // Último tramo parcial.
        steps.push(Step::new(&eb.id, end_fwd));
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
    for (i, j) in pairs {
        let (l, pts) = leg(g, &adj, &snaps[i], &snaps[j], opts)?;
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
        waypoints: waypoints.to_vec(),
        legs,
        length_m,
        en,
        geo,
    })
}

impl ResolvedRoute {
    /// Fragmento `route` del descriptor de pista (documento de arquitectura v0.2).
    pub fn descriptor_route(&self) -> serde_json::Value {
        let edges: Vec<&str> = self.legs.iter().flat_map(|l| l.steps.iter().map(|s| s.0.as_str())).collect();
        serde_json::json!({
            "waypoints": self.waypoints,
            "resolvedPath": {
                "graph": format!("{}@{}", self.graph_region, self.graph_hash),
                "respectOneway": self.respect_oneway,
                "edges": edges,
            },
            "closed": self.closed,
            "lengthM": (self.length_m * 10.0).round() / 10.0,
        })
    }
}
