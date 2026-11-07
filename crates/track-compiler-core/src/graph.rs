//! Grafo vial dirigido a partir de un extracto de OSM.
//!
//! - Solo se incluyen vías conducibles por autos (ver `RoadClass`).
//! - Dos vías se conectan **solo si comparten un nodo**. Un puente que cruza una avenida
//!   no comparte nodo con ella, así que nunca se unen aunque sus líneas se crucen en el mapa.
//! - Cada vía se corta en aristas en los nodos de cruce y en sus extremos.

use crate::diagnostics::{Diagnostic, Severity};
use crate::geo::{GeoPoint, LocalFrame};
use crate::hash::Fnv64;
use crate::osm::{Extract, Tags, Way};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const GRAPH_FORMAT: &str = "road-graph/1";

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoadClass {
    Motorway,
    Trunk,
    Primary,
    Secondary,
    Tertiary,
    Unclassified,
    Residential,
    LivingStreet,
    MotorwayLink,
    TrunkLink,
    PrimaryLink,
    SecondaryLink,
    TertiaryLink,
}

impl RoadClass {
    pub fn from_tag(v: &str) -> Option<Self> {
        Some(match v {
            "motorway" => Self::Motorway,
            "trunk" => Self::Trunk,
            "primary" => Self::Primary,
            "secondary" => Self::Secondary,
            "tertiary" => Self::Tertiary,
            "unclassified" => Self::Unclassified,
            "residential" => Self::Residential,
            "living_street" => Self::LivingStreet,
            "motorway_link" => Self::MotorwayLink,
            "trunk_link" => Self::TrunkLink,
            "primary_link" => Self::PrimaryLink,
            "secondary_link" => Self::SecondaryLink,
            "tertiary_link" => Self::TertiaryLink,
            // service, track, footway, cycleway, path, pedestrian, steps… quedan fuera en S1.
            _ => return None,
        })
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Motorway => "motorway",
            Self::Trunk => "trunk",
            Self::Primary => "primary",
            Self::Secondary => "secondary",
            Self::Tertiary => "tertiary",
            Self::Unclassified => "unclassified",
            Self::Residential => "residential",
            Self::LivingStreet => "living_street",
            Self::MotorwayLink => "motorway_link",
            Self::TrunkLink => "trunk_link",
            Self::PrimaryLink => "primary_link",
            Self::SecondaryLink => "secondary_link",
            Self::TertiaryLink => "tertiary_link",
        }
    }

    fn is_link(self) -> bool {
        matches!(self, Self::MotorwayLink | Self::TrunkLink | Self::PrimaryLink | Self::SecondaryLink | Self::TertiaryLink)
    }

    /// Carriles totales cuando OSM no lo dice.
    pub fn default_lanes(self, oneway: bool) -> u32 {
        match self {
            Self::Motorway | Self::Trunk => {
                if oneway {
                    3
                } else {
                    4
                }
            }
            Self::Primary => {
                if oneway {
                    3
                } else {
                    4
                }
            }
            Self::Secondary | Self::Tertiary | Self::Unclassified | Self::Residential => 2,
            Self::LivingStreet => 1,
            _ if self.is_link() => 1,
            _ => 2,
        }
    }

    pub fn lane_width(self) -> f64 {
        match self {
            Self::Motorway | Self::Trunk | Self::MotorwayLink | Self::TrunkLink => 3.5,
            Self::LivingStreet => 3.0,
            _ => 3.25,
        }
    }
}

/// Sentido permitido respecto del orden de los puntos de la arista.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Oneway {
    Both,
    Forward,
    Backward,
}

impl Oneway {
    pub fn allows(self, forward: bool) -> bool {
        match self {
            Oneway::Both => true,
            Oneway::Forward => forward,
            Oneway::Backward => !forward,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Vertex {
    pub osm_id: i64,
    pub lat: f64,
    pub lon: f64,
    pub en: [f64; 2],
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Edge {
    /// Estable dentro de un snapshot: "w<id de vía>#<tramo>".
    pub id: String,
    pub way: i64,
    pub from: usize,
    pub to: usize,
    pub class: RoadClass,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub oneway: Oneway,
    pub lanes: u32,
    pub width_m: f64,
    pub width_estimated: bool,
    pub bridge: bool,
    pub tunnel: bool,
    pub layer: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub maxspeed_kmh: Option<f64>,
    pub length_m: f64,
    /// Geometría en el plano local, de `from` a `to`.
    pub en: Vec<[f64; 2]>,
    /// Misma geometría en grados, para exportar y verificar.
    pub geo: Vec<GeoPoint>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceInfo {
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub osm_base_timestamp: Option<String>,
    pub attribution: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComponentInfo {
    pub count: usize,
    pub largest_vertices: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoadGraph {
    pub format: String,
    pub compiler_version: String,
    pub region: String,
    pub frame: LocalFrame,
    pub source: SourceInfo,
    pub vertices: Vec<Vertex>,
    pub edges: Vec<Edge>,
    /// Componente (conectividad sin importar sentidos) de cada vértice.
    pub component: Vec<u32>,
    pub components: ComponentInfo,
    pub diagnostics: Vec<Diagnostic>,
    pub hash: String,
}

#[derive(Clone, Debug)]
pub struct BuildOptions {
    pub region: String,
    /// Origen del marco local. Si falta, el centro de la caja de nodos (redondeado a 1e-4°).
    pub origin: Option<GeoPoint>,
}

struct WayAttrs {
    class: RoadClass,
    name: Option<String>,
    oneway: Oneway,
    lanes: u32,
    width_m: f64,
    width_estimated: bool,
    bridge: bool,
    tunnel: bool,
    layer: i32,
    maxspeed_kmh: Option<f64>,
}

fn yes(tags: &Tags, k: &str) -> bool {
    tags.get(k).is_some_and(|v| v != "no" && v != "0" && v != "false")
}

fn parse_meters(v: &str) -> Option<f64> {
    let t = v.trim().trim_end_matches('m').trim().replace(',', ".");
    t.parse::<f64>().ok().filter(|x| *x > 0.5 && *x < 80.0)
}

fn parse_speed(v: &str) -> Option<f64> {
    let t = v.trim();
    if let Some(mph) = t.strip_suffix("mph") {
        return mph.trim().parse::<f64>().ok().map(|x| x * 1.609_344);
    }
    t.parse::<f64>().ok()
}

/// Atributos de una vía, o `None` si no es conducible.
fn attrs(way: &Way) -> Result<WayAttrs, &'static str> {
    let tags = &way.tags;
    let class = tags.get("highway").and_then(|h| RoadClass::from_tag(h)).ok_or("no-drivable")?;
    if yes(tags, "area") {
        return Err("area");
    }
    let access_blocked = |k: &str| tags.get(k).is_some_and(|v| v == "no" || v == "private");
    let motor_ok = tags.get("motor_vehicle").is_some_and(|v| v == "yes" || v == "permissive")
        || tags.get("motorcar").is_some_and(|v| v == "yes");
    if (access_blocked("access") || access_blocked("motor_vehicle") || access_blocked("vehicle")) && !motor_ok {
        return Err("access");
    }

    let roundabout = tags.get("junction").is_some_and(|j| j == "roundabout" || j == "circular");
    let oneway = match tags.get("oneway").map(String::as_str) {
        Some("yes" | "1" | "true") => Oneway::Forward,
        Some("-1" | "reverse") => Oneway::Backward,
        Some("no" | "0" | "false") => Oneway::Both,
        _ if roundabout || matches!(class, RoadClass::Motorway | RoadClass::MotorwayLink) => Oneway::Forward,
        _ => Oneway::Both,
    };

    let lanes_tag = tags.get("lanes").and_then(|l| l.split(';').next()?.trim().parse::<u32>().ok()).filter(|l| (1..=12).contains(l));
    let lanes = lanes_tag.unwrap_or_else(|| class.default_lanes(oneway != Oneway::Both));
    let width_tag = tags.get("width").and_then(|w| parse_meters(w));
    let (width_m, width_estimated) = match width_tag {
        Some(w) => (w, false),
        None => (lanes as f64 * class.lane_width(), true),
    };

    let bridge = yes(tags, "bridge");
    let tunnel = yes(tags, "tunnel") && tags.get("tunnel").map(String::as_str) != Some("building_passage");
    let layer = tags
        .get("layer")
        .and_then(|l| l.trim().parse::<i32>().ok())
        .unwrap_or(if bridge { 1 } else if tunnel { -1 } else { 0 });

    Ok(WayAttrs {
        class,
        name: tags.get("name").cloned(),
        oneway,
        lanes,
        width_m,
        width_estimated,
        bridge,
        tunnel,
        layer,
        maxspeed_kmh: tags.get("maxspeed").and_then(|v| parse_speed(v)),
    })
}

fn polyline_length(pts: &[[f64; 2]]) -> f64 {
    pts.windows(2).map(|w| libm::hypot(w[1][0] - w[0][0], w[1][1] - w[0][1])).sum()
}

impl RoadGraph {
    pub fn build(ex: &Extract, opts: &BuildOptions) -> Result<Self, String> {
        let mut diags: Vec<Diagnostic> = Vec::new();

        // 1. Vías conducibles y sus atributos.
        let mut kept: Vec<(&Way, WayAttrs)> = Vec::new();
        let mut skipped: BTreeMap<&'static str, usize> = BTreeMap::new();
        for w in &ex.ways {
            match attrs(w) {
                Ok(a) => kept.push((w, a)),
                Err(reason) => *skipped.entry(reason).or_default() += 1,
            }
        }
        if kept.is_empty() {
            return Err("el extracto no tiene vías conducibles por autos".into());
        }
        if let Some(n) = skipped.get("access") {
            diags.push(Diagnostic::new(Severity::Info, "access-excluded", format!("{n} vías con acceso restringido quedaron fuera")));
        }

        // 2. Marco local.
        let origin = match opts.origin {
            Some(o) => o,
            None => {
                let (mut lo_lat, mut hi_lat, mut lo_lon, mut hi_lon) = (f64::MAX, f64::MIN, f64::MAX, f64::MIN);
                for (w, _) in &kept {
                    for id in &w.nodes {
                        if let Some(n) = ex.nodes.get(id) {
                            lo_lat = lo_lat.min(n.lat);
                            hi_lat = hi_lat.max(n.lat);
                            lo_lon = lo_lon.min(n.lon);
                            hi_lon = hi_lon.max(n.lon);
                        }
                    }
                }
                let round = |x: f64| libm::round(x * 1e4) / 1e4;
                GeoPoint::new(round((lo_lat + hi_lat) / 2.0), round((lo_lon + hi_lon) / 2.0))
            }
        };
        let frame = LocalFrame::new(origin);

        // 3. Partir vías en tramos continuos (un nodo faltante corta la vía).
        let mut pieces: Vec<(usize, Vec<i64>)> = Vec::new(); // (índice en kept, nodos)
        for (k, (w, _)) in kept.iter().enumerate() {
            let mut cur: Vec<i64> = Vec::new();
            let mut missing = 0;
            for id in &w.nodes {
                if ex.nodes.contains_key(id) {
                    cur.push(*id);
                } else {
                    missing += 1;
                    if cur.len() >= 2 {
                        pieces.push((k, std::mem::take(&mut cur)));
                    }
                    cur.clear();
                }
            }
            if cur.len() >= 2 {
                pieces.push((k, cur));
            }
            if missing > 0 {
                diags.push(
                    Diagnostic::new(Severity::Warning, "missing-nodes", format!("la vía referencia {missing} nodos que no están en el extracto; se cortó"))
                        .way(w.id),
                );
            }
        }

        // 4. Nodos de cruce: extremos de tramo o nodos usados más de una vez.
        let mut uses: BTreeMap<i64, u32> = BTreeMap::new();
        for (_, nodes) in &pieces {
            for id in nodes {
                *uses.entry(*id).or_default() += 1;
            }
        }
        let mut is_vertex: BTreeMap<i64, bool> = BTreeMap::new();
        for (_, nodes) in &pieces {
            for (i, id) in nodes.iter().enumerate() {
                let end = i == 0 || i + 1 == nodes.len();
                if end || uses[id] > 1 {
                    is_vertex.insert(*id, true);
                }
            }
        }
        let mut vertex_ids: Vec<i64> = is_vertex.keys().copied().collect();
        vertex_ids.sort_unstable();
        let index: BTreeMap<i64, usize> = vertex_ids.iter().enumerate().map(|(i, id)| (*id, i)).collect();
        let vertices: Vec<Vertex> = vertex_ids
            .iter()
            .map(|id| {
                let n = &ex.nodes[id];
                let g = GeoPoint::new(n.lat, n.lon);
                Vertex { osm_id: *id, lat: n.lat, lon: n.lon, en: frame.to_en(g) }
            })
            .collect();

        // 5. Aristas entre nodos de cruce consecutivos.
        let mut edges: Vec<Edge> = Vec::new();
        let mut seg_counter: BTreeMap<i64, u32> = BTreeMap::new();
        let mut estimated = 0usize;
        for (k, nodes) in &pieces {
            let (w, a) = &kept[*k];
            let mut start = 0usize;
            for i in 1..nodes.len() {
                if !is_vertex.contains_key(&nodes[i]) {
                    continue;
                }
                let slice = &nodes[start..=i];
                start = i;
                if slice.len() < 2 || (slice.len() == 2 && slice[0] == slice[1]) {
                    continue;
                }
                let geo: Vec<GeoPoint> = slice.iter().map(|id| GeoPoint::new(ex.nodes[id].lat, ex.nodes[id].lon)).collect();
                let en: Vec<[f64; 2]> = geo.iter().map(|g| frame.to_en(*g)).collect();
                let length_m = polyline_length(&en);
                if length_m < 0.05 {
                    continue;
                }
                let seg = seg_counter.entry(w.id).or_default();
                let id = format!("w{}#{}", w.id, seg);
                *seg += 1;
                if a.width_estimated {
                    estimated += 1;
                }
                edges.push(Edge {
                    id,
                    way: w.id,
                    from: index[&slice[0]],
                    to: index[&slice[slice.len() - 1]],
                    class: a.class,
                    name: a.name.clone(),
                    oneway: a.oneway,
                    lanes: a.lanes,
                    width_m: a.width_m,
                    width_estimated: a.width_estimated,
                    bridge: a.bridge,
                    tunnel: a.tunnel,
                    layer: a.layer,
                    maxspeed_kmh: a.maxspeed_kmh,
                    length_m,
                    en,
                    geo,
                });
            }
        }
        if estimated > 0 {
            diags.push(Diagnostic::new(
                Severity::Info,
                "width-estimated",
                format!("{estimated} de {} aristas no tienen ancho en OSM; se estimó por carriles y tipo de vía", edges.len()),
            ));
        }
        if ex.relations > 0 {
            diags.push(Diagnostic::new(
                Severity::Info,
                "relations-ignored",
                format!("{} relaciones ignoradas (incluye restricciones de giro, que llegan con S3)", ex.relations),
            ));
        }

        // 6. Componentes conexos sin importar sentidos (union-find).
        let mut parent: Vec<usize> = (0..vertices.len()).collect();
        fn find(p: &mut [usize], mut x: usize) -> usize {
            while p[x] != x {
                p[x] = p[p[x]];
                x = p[x];
            }
            x
        }
        for e in &edges {
            let (a, b) = (find(&mut parent, e.from), find(&mut parent, e.to));
            if a != b {
                parent[a.max(b)] = a.min(b);
            }
        }
        let roots: Vec<usize> = (0..vertices.len()).map(|v| find(&mut parent, v)).collect();
        let mut sizes: BTreeMap<usize, usize> = BTreeMap::new();
        for r in &roots {
            *sizes.entry(*r).or_default() += 1;
        }
        // Numera componentes de mayor a menor (desempate por raíz, determinista).
        let mut order: Vec<(usize, usize)> = sizes.iter().map(|(r, n)| (*r, *n)).collect();
        order.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        let comp_of: BTreeMap<usize, u32> = order.iter().enumerate().map(|(i, (r, _))| (*r, i as u32)).collect();
        let component: Vec<u32> = roots.iter().map(|r| comp_of[r]).collect();
        let components = ComponentInfo { count: order.len(), largest_vertices: order.first().map_or(0, |x| x.1) };
        if components.count > 1 {
            diags.push(Diagnostic::new(
                Severity::Info,
                "disconnected",
                format!(
                    "{} componentes desconectados; el mayor tiene {} de {} nodos. Las rutas solo existen dentro de un mismo componente.",
                    components.count,
                    components.largest_vertices,
                    vertices.len()
                ),
            ));
        }

        let mut g = RoadGraph {
            format: GRAPH_FORMAT.into(),
            compiler_version: crate::COMPILER_VERSION.into(),
            region: opts.region.clone(),
            frame,
            source: SourceInfo {
                kind: "overpass".into(),
                osm_base_timestamp: ex.osm_base_timestamp.clone(),
                attribution: "© OpenStreetMap contributors, ODbL".into(),
            },
            vertices,
            edges,
            component,
            components,
            diagnostics: diags,
            hash: String::new(),
        };
        g.hash = g.compute_hash();
        Ok(g)
    }

    /// Hash canónico del contenido (no incluye diagnósticos ni el propio hash).
    pub fn compute_hash(&self) -> String {
        let mut h = Fnv64::default();
        h.str_u64_len(GRAPH_FORMAT).str_u64_len(&self.compiler_version).str_u64_len(&self.region);
        h.f64(self.frame.origin.lat).f64(self.frame.origin.lon);
        for v in &self.vertices {
            h.i64(v.osm_id).f64(v.lat).f64(v.lon);
        }
        for e in &self.edges {
            h.str_u64_len(&e.id).u64(e.from as u64).u64(e.to as u64).str_u64_len(e.class.as_str());
            h.u64(match e.oneway {
                Oneway::Both => 0,
                Oneway::Forward => 1,
                Oneway::Backward => 2,
            });
            h.f64(e.width_m).u64(e.lanes as u64).u64(e.bridge as u64).u64(e.tunnel as u64).i64(e.layer as i64);
            for p in &e.geo {
                h.f64(p.lat).f64(p.lon);
            }
        }
        h.hex()
    }

    pub fn from_json(s: &str) -> Result<Self, String> {
        let g: RoadGraph = serde_json::from_str(s).map_err(|e| format!("grafo inválido: {e}"))?;
        if g.format != GRAPH_FORMAT {
            return Err(format!("formato de grafo desconocido: {}", g.format));
        }
        if g.compute_hash() != g.hash {
            return Err("el hash del grafo no coincide con su contenido (¿se editó a mano?)".into());
        }
        Ok(g)
    }

    pub fn edge_index(&self, id: &str) -> Option<usize> {
        self.edges.iter().position(|e| e.id == id)
    }

    /// Aristas salientes de cada vértice: (arista, avanza en el sentido de su geometría).
    pub fn adjacency(&self, respect_oneway: bool) -> Vec<Vec<(usize, bool)>> {
        let mut adj = vec![Vec::new(); self.vertices.len()];
        for (i, e) in self.edges.iter().enumerate() {
            if !respect_oneway || e.oneway.allows(true) {
                adj[e.from].push((i, true));
            }
            if !respect_oneway || e.oneway.allows(false) {
                adj[e.to].push((i, false));
            }
        }
        adj
    }

    pub fn total_length_km(&self) -> f64 {
        self.edges.iter().map(|e| e.length_m).sum::<f64>() / 1000.0
    }
}
