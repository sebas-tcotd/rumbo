//! Pruebas de S3: red conducible con cruces (San Borja, sector Las Artes Norte).
use std::collections::BTreeMap;
use track_compiler::dem::Dem;
use track_compiler::graph::{BuildOptions, RoadGraph};
use track_compiler::network::{self, NetworkOptions, ASPHALT};
use track_compiler::osm::Extract;
use track_compiler::routing::{route, ResolvedRoute, RouteOptions};
use track_compiler::trackgen::{generate, Status, TrackOptions};
use track_compiler::GeoPoint;

const DEM: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/raw/Copernicus_DSM_COG_10_S13_00_W077_00_DEM.tif");
const OSM: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/raw/lima-san-borja-2026-11-04.overpass.json");
const ASSET: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/tracks/san-borja-las-artes.json");

fn have_data() -> bool {
    let ok = std::path::Path::new(DEM).exists() && std::path::Path::new(OSM).exists();
    if !ok {
        eprintln!("faltan los datos de data/raw: se omite");
    }
    ok
}

fn graph() -> RoadGraph {
    let ex = Extract::from_overpass_json(&std::fs::read_to_string(OSM).unwrap()).unwrap();
    RoadGraph::build(&ex, &BuildOptions { region: "lima-san-borja".into(), origin: None }).unwrap()
}

/// Receta de la pista de `assets/`, leída de su propio bloque `source`.
fn recipe() -> (Vec<GeoPoint>, RouteOptions, TrackOptions) {
    let a: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(ASSET).unwrap()).unwrap();
    let src = &a["source"];
    let pts: Vec<GeoPoint> = serde_json::from_value(src["route"]["waypoints"].clone()).unwrap();
    let path = &src["route"]["resolvedPath"];
    let ropts = RouteOptions {
        respect_oneway: path["respectOneway"].as_bool().unwrap(),
        closed: src["route"]["closed"].as_bool().unwrap(),
        max_turn_deg: path["maxTurnDeg"].as_f64().unwrap(),
        turn_cost_m: path["turnCostM"].as_f64().unwrap(),
    };
    let mut t = TrackOptions::new(a["id"].as_str().unwrap(), a["name"].as_str().unwrap());
    let n = &src["network"];
    t.network = Some(NetworkOptions {
        sidewalk_m: n["sidewalkM"].as_f64().unwrap(),
        curb_radius_m: n["curbRadiusM"].as_f64().unwrap(),
        stub_m: n["stubM"].as_f64().unwrap(),
        ..NetworkOptions::default()
    });
    let tr = &n["terrain"];
    t.terrain.step_m = tr["stepM"].as_f64().unwrap();
    t.terrain.radius_m = tr["radiusM"].as_f64().unwrap();
    t.terrain.percentile = tr["percentile"].as_f64().unwrap();
    t.terrain.sigma_m = tr["sigmaM"].as_f64().unwrap();
    (pts, ropts, t)
}

fn circuit(g: &RoadGraph) -> ResolvedRoute {
    let (pts, opts, _) = recipe();
    route(g, &pts, opts).unwrap()
}

#[test]
fn junctions_are_found_and_classified() {
    if !have_data() {
        return;
    }
    let g = graph();
    let net = network::build(&g, &circuit(&g), &NetworkOptions::default()).unwrap();
    let kinds: BTreeMap<String, usize> = net.junctions.iter().fold(BTreeMap::new(), |mut m, j| {
        *m.entry(j.kind.clone()).or_default() += 1;
        m
    });
    assert_eq!(kinds.get("T"), Some(&5), "{kinds:?}");
    assert_eq!(kinds.get("cruce"), Some(&2), "{kinds:?}");
    assert!(net.junctions.iter().all(|j| j.status == Status::Valid), "{:?}", net.junctions.iter().map(|j| (&j.kind, j.status, &j.reason)).collect::<Vec<_>>());
    // Los dos cruces que eligió el usuario: Monet × Rousseau (T) y Milet × Rousseau (cruce).
    let at = |lat: f64, lon: f64| {
        let p = g.frame.to_en(GeoPoint::new(lat, lon));
        net.junctions.iter().find(|j| libm::hypot(j.en[0] - p[0], j.en[1] - p[1]) < 3.0).map(|j| j.kind.clone())
    };
    assert_eq!(at(-12.090049, -76.990954).as_deref(), Some("T"));
    assert_eq!(at(-12.090938, -76.991168).as_deref(), Some("cruce"));
}

fn on_segment(p: [f64; 2], a: [f64; 2], b: [f64; 2]) -> bool {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let l2 = dx * dx + dy * dy;
    if l2 < 1e-12 {
        return libm::hypot(p[0] - a[0], p[1] - a[1]) < 1e-6;
    }
    let t = ((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / l2;
    let off = ((p[0] - a[0]) * dy - (p[1] - a[1]) * dx).abs() / l2.sqrt();
    (-1e-9..=1.0 + 1e-9).contains(&t) && off < 1e-6
}

/// La superficie no tiene huecos: cada arista que no comparten dos triángulos es borde
/// exterior y está cubierta por un muro. Ningún triángulo queda al revés ni superpuesto
/// en una arista (más de dos triángulos por arista).
#[test]
fn surface_is_watertight_and_walled() {
    if !have_data() {
        return;
    }
    let g = graph();
    let net = network::build(&g, &circuit(&g), &NetworkOptions::default()).unwrap();
    let mut uses: BTreeMap<(u32, u32), usize> = BTreeMap::new();
    for (t, _) in &net.triangles {
        let [a, b, c] = *t;
        let p = |i: u32| net.vertices[i as usize];
        let area = (p(b)[0] - p(a)[0]) * (p(c)[1] - p(a)[1]) - (p(b)[1] - p(a)[1]) * (p(c)[0] - p(a)[0]);
        assert!(area > 0.0, "triángulo al revés");
        for (u, v) in [(a, b), (b, c), (c, a)] {
            *uses.entry((u.min(v), u.max(v))).or_default() += 1;
        }
    }
    assert!(uses.values().all(|&n| n <= 2), "una arista con más de dos triángulos: superficie superpuesta");
    let walls: Vec<([f64; 2], [f64; 2])> = net.walls.iter().flat_map(|w| w.windows(2).map(|s| (s[0], s[1]))).collect();
    let boundary: Vec<(u32, u32)> = uses.iter().filter(|(_, &n)| n == 1).map(|(&e, _)| e).collect();
    let open: Vec<_> = boundary
        .iter()
        .filter(|(u, v)| {
            let (a, b) = (net.vertices[*u as usize], net.vertices[*v as usize]);
            !walls.iter().any(|&(p, q)| on_segment(a, p, q) && on_segment(b, p, q))
        })
        .map(|(u, _)| net.vertices[*u as usize])
        .collect();
    assert!(open.is_empty(), "{} bordes sin muro (huecos en la superficie), p. ej. en {:?}", open.len(), open.first());
}

/// La línea de referencia (cronómetro y piloto automático) va siempre sobre la calzada.
#[test]
fn reference_line_stays_on_the_road() {
    if !have_data() {
        return;
    }
    let g = graph();
    let net = network::build(&g, &circuit(&g), &NetworkOptions::default()).unwrap();
    let road: Vec<[[f64; 2]; 3]> = net
        .triangles
        .iter()
        .filter(|(_, s)| *s == ASPHALT)
        .map(|(t, _)| t.map(|i| net.vertices[i as usize]))
        .collect();
    let inside = |p: [f64; 2]| {
        road.iter().any(|t| {
            let c = |a: [f64; 2], b: [f64; 2]| (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0]);
            c(t[0], t[1]) >= -1e-9 && c(t[1], t[2]) >= -1e-9 && c(t[2], t[0]) >= -1e-9
        })
    };
    let off: Vec<_> = net.reference.iter().filter(|q| !inside(q.en)).collect();
    assert!(off.is_empty(), "{} puntos de la referencia fuera de la calzada", off.len());
    let l: f64 = net.reference.windows(2).map(|w| libm::hypot(w[1].en[0] - w[0].en[0], w[1].en[1] - w[0].en[1])).sum();
    assert!((600.0..700.0).contains(&l), "largo de la referencia {l}");
}

#[test]
fn generated_track_is_valid_and_gentle() {
    if !have_data() {
        return;
    }
    let g = graph();
    let (_, _, opts) = recipe();
    let t = generate(&g, &circuit(&g), &Dem::from_path(DEM).unwrap(), &opts).unwrap();
    assert_eq!(t.status, Status::Valid);
    assert!(t.profile.max_grade < 0.06, "San Borja es casi plano: pendiente {}", t.profile.max_grade);
    let n = &t.json["network"];
    assert_eq!(n["triangles"].as_array().unwrap().len(), n["surfaces"].as_array().unwrap().len());
}

/// La pista de `assets/` debe ser exactamente la que produce el compilador actual.
#[test]
fn committed_asset_is_up_to_date() {
    if !have_data() || !std::path::Path::new(ASSET).exists() {
        return;
    }
    let g = graph();
    let (_, _, opts) = recipe();
    let t = generate(&g, &circuit(&g), &Dem::from_path(DEM).unwrap(), &opts).unwrap();
    let committed: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(ASSET).unwrap()).unwrap();
    assert_eq!(committed, t.json, "assets/tracks/san-borja-las-artes.json está desactualizada: regénerala");
}
