//! Pruebas de S1: proyección, grafo y ruteo sobre el fixture sintético.
use track_compiler_core::export::{graph_geojson, route_geojson};
use track_compiler_core::geo::{curvature_radii, haversine_m};
use track_compiler_core::graph::{BuildOptions, Oneway, RoadGraph};
use track_compiler_core::osm::Extract;
use track_compiler_core::routing::{route, RouteOptions};
use track_compiler_core::{GeoPoint, LocalFrame, Severity};

// Misma conversión aproximada que generar_fixture.py.
const LAT0: f64 = -12.12;
const LON0: f64 = -77.03;
fn geo(e: f64, n: f64) -> GeoPoint {
    GeoPoint::new(LAT0 + n / 110_580.0, LON0 + e / 108_840.0)
}

fn graph() -> RoadGraph {
    let ex = Extract::from_overpass_json(include_str!("../../compiler-cli/tests/fixtures/mini-grid.overpass.json")).unwrap();
    RoadGraph::build(&ex, &BuildOptions { region: "fixture".into(), origin: Some(GeoPoint::new(LAT0, LON0)) }).unwrap()
}

fn close(a: f64, b: f64, tol: f64) -> bool {
    (a - b).abs() <= tol
}

// ── Proyección ──

#[test]
fn projection_is_east_north_in_meters() {
    let f = LocalFrame::new(GeoPoint::new(LAT0, LON0));
    let o = f.to_enu(GeoPoint::new(LAT0, LON0));
    assert!(o.iter().all(|v| v.abs() < 1e-6), "el origen debe proyectarse en 0: {o:?}");
    let east = GeoPoint::new(LAT0, LON0 + 0.001);
    let north = GeoPoint::new(LAT0 + 0.001, LON0);
    let pe = f.to_en(east);
    let pn = f.to_en(north);
    assert!(pe[0] > 100.0 && pe[1].abs() < 0.01, "más longitud debe ser más al este: {pe:?}");
    assert!(pn[1] > 100.0 && pn[0].abs() < 0.01, "más latitud debe ser más al norte: {pn:?}");
    // Contra los radios de curvatura del elipsoide WGS84 (no contra una esfera:
    // cerca del ecuador la esfera sobrestima el arco norte-sur en ~0,5 %).
    let (m, n) = curvature_radii(LAT0);
    let step = 0.001_f64.to_radians();
    let (ee, en) = (n * LAT0.to_radians().cos() * step, m * step);
    assert!((pe[0] - ee).abs() / ee < 1e-4, "este {} vs {}", pe[0], ee);
    assert!((pn[1] - en).abs() / en < 1e-4, "norte {} vs {}", pn[1], en);
    // Y la esfera media queda dentro del 1 %, como verificación gruesa.
    let hn = haversine_m(GeoPoint::new(LAT0, LON0), north);
    assert!((pn[1] - hn).abs() / hn < 0.01);
}

#[test]
fn projection_round_trips() {
    let f = LocalFrame::new(GeoPoint::new(LAT0, LON0));
    for p in [geo(1500.0, -800.0), geo(-3000.0, 2500.0), geo(4.2, 7.7)] {
        let back = f.en_to_geo(f.to_en(p));
        assert!(close(back.lat, p.lat, 1e-9) && close(back.lon, p.lon, 1e-9), "{p:?} → {back:?}");
    }
}

#[test]
fn lat_lon_order_is_checked() {
    assert!(GeoPoint::parse("-12.12,-77.03").is_ok());
    // Fuera de rango: se rechaza al leer.
    assert!(GeoPoint::parse("-12.12,-200").is_err());
    assert!(GeoPoint::parse("120,-12.12").unwrap_err().contains("invertido"));
    // Invertido pero dentro de rango (la longitud de Lima cabe como latitud):
    // lo detecta el ajuste a la red vial, que conoce la región.
    let inverted = GeoPoint::parse("-77.03,-12.12").unwrap();
    let err = route(&graph(), &[inverted, geo(200.0, 0.0)], RouteOptions::default()).unwrap_err();
    assert!(err.contains("invertido"), "{err}");
}

// ── Grafo ──

#[test]
fn graph_splits_ways_only_at_shared_nodes() {
    let g = graph();
    // 9 cruces de la cuadrícula + 2 extremos del puente.
    assert_eq!(g.vertices.len(), 11);
    // 3 calles H y 3 V partidas en su cruce central + el puente entero.
    assert_eq!(g.edges.len(), 13);
    assert!(g.edges.iter().any(|e| e.id == "w10#0") && g.edges.iter().any(|e| e.id == "w10#1"));
    // Los nodos intermedios de H0 (en e=50 y e=150) no son cruces.
    let h0 = g.edges.iter().find(|e| e.id == "w10#0").unwrap();
    assert_eq!(h0.geo.len(), 3);
    assert!(close(h0.length_m, 100.0, 0.5), "{}", h0.length_m);
}

#[test]
fn bridge_crossing_streets_is_not_connected() {
    let g = graph();
    let bridge = g.edges.iter().find(|e| e.way == 40).expect("puente");
    assert!(bridge.bridge && bridge.layer == 1);
    assert_eq!(bridge.maxspeed_kmh, Some(60.0));
    let grid_component = g.component[g.edges.iter().find(|e| e.way == 10).unwrap().from];
    assert_ne!(g.component[bridge.from], grid_component, "el puente no comparte nodos con las calles que cruza");
    assert_eq!(g.components.count, 2);
}

#[test]
fn non_drivable_and_restricted_ways_are_excluded() {
    let g = graph();
    assert!(!g.edges.iter().any(|e| e.way == 30), "la peatonal no es conducible");
    assert!(!g.edges.iter().any(|e| e.way == 60), "la calle privada queda fuera");
    assert!(g.diagnostics.iter().any(|d| d.code == "access-excluded"));
}

#[test]
fn missing_nodes_produce_a_located_warning() {
    let g = graph();
    let d = g.diagnostics.iter().find(|d| d.code == "missing-nodes").expect("aviso de nodos faltantes");
    assert_eq!(d.severity, Severity::Warning);
    assert_eq!(d.way, Some(50));
}

#[test]
fn attributes_and_estimates() {
    let g = graph();
    let v1 = g.edges.iter().filter(|e| e.way == 21).collect::<Vec<_>>();
    assert!(v1.iter().all(|e| e.oneway == Oneway::Forward));
    let h2 = g.edges.iter().find(|e| e.way == 12).unwrap();
    assert!(!h2.width_estimated && close(h2.width_m, 7.5, 1e-9));
    let h0 = g.edges.iter().find(|e| e.way == 10).unwrap();
    assert!(h0.width_estimated && close(h0.width_m, 6.5, 1e-9), "2 carriles × 3,25 m");
    let info = g.diagnostics.iter().find(|d| d.code == "width-estimated").unwrap();
    assert_eq!(info.severity, Severity::Info, "un ancho estimado informa, no degrada");
}

#[test]
fn graph_hash_is_stable_and_verified_on_load() {
    let a = graph();
    let b = graph();
    assert_eq!(a.hash, b.hash);
    let json = serde_json::to_string(&a).unwrap();
    let back = RoadGraph::from_json(&json).unwrap();
    assert_eq!(back.hash, a.hash);
    assert_eq!(back, a);
    let tampered = json.replacen("\"widthM\":7.5", "\"widthM\":9.5", 1);
    assert!(RoadGraph::from_json(&tampered).is_err(), "un grafo editado a mano debe rechazarse");
}

// ── Ruteo ──

#[test]
fn shortest_route_ignores_the_footway_shortcut() {
    let g = graph();
    let r = route(&g, &[geo(0.0, 0.0), geo(200.0, 200.0)], RouteOptions::default()).unwrap();
    assert!(close(r.length_m, 400.0, 1.5), "la peatonal en diagonal mediría 283 m: {}", r.length_m);
}

#[test]
fn oneway_is_ignored_for_races_and_respected_on_request() {
    let g = graph();
    let pts = [geo(100.0, 200.0), geo(100.0, 0.0)]; // V1 hacia el sur, contra su sentido
    let free = route(&g, &pts, RouteOptions::default()).unwrap();
    assert!(close(free.length_m, 200.0, 1.0), "{}", free.length_m);
    let legal = route(&g, &pts, RouteOptions { respect_oneway: true, ..RouteOptions::default() }).unwrap();
    assert!(close(legal.length_m, 400.0, 1.5), "debe rodear por V0 o V2: {}", legal.length_m);
    assert!(legal.legs[0].steps.iter().all(|s| !s.0.contains("w21")), "no debe usar V1 al revés");
}

#[test]
fn route_within_a_single_edge() {
    let g = graph();
    let r = route(&g, &[geo(10.0, 3.0), geo(60.0, -2.0)], RouteOptions::default()).unwrap();
    assert!(close(r.length_m, 50.0, 0.5), "{}", r.length_m);
    assert_eq!(r.legs[0].steps.len(), 1);
    assert!(close(r.legs[0].from.offset_m, 3.0, 0.1));
}

#[test]
fn closed_circuit_returns_to_the_start() {
    let g = graph();
    let r = route(&g, &[geo(0.0, 0.0), geo(200.0, 0.0), geo(200.0, 200.0)], RouteOptions { closed: true, ..RouteOptions::default() }).unwrap();
    assert_eq!(r.legs.len(), 3);
    assert!(close(r.length_m, 800.0, 3.0), "{}", r.length_m);
    let (a, b) = (r.geo.first().unwrap(), r.geo.last().unwrap());
    assert!(haversine_m(*a, *b) < 0.5, "el circuito debe cerrar");
}

#[test]
fn unreachable_or_far_points_explain_why() {
    let g = graph();
    let far = route(&g, &[geo(0.0, 0.0), geo(0.0, -500.0)], RouteOptions::default()).unwrap_err();
    assert!(far.contains("calle más cercana"), "{far}");
    let cut = route(&g, &[geo(0.0, 0.0), geo(50.0, -55.0)], RouteOptions::default()).unwrap_err();
    assert!(cut.contains("no se conectan"), "{cut}");
}

#[test]
fn route_is_deterministic_and_describes_the_resolved_path() {
    let g = graph();
    let pts = [geo(5.0, 5.0), geo(195.0, 105.0)];
    let a = route(&g, &pts, RouteOptions::default()).unwrap();
    let b = route(&g, &pts, RouteOptions::default()).unwrap();
    assert_eq!(a, b);
    let d = a.descriptor_route();
    assert_eq!(d["resolvedPath"]["graph"], format!("fixture@{}", g.hash));
    let edges = d["resolvedPath"]["edges"].as_array().unwrap();
    assert!(!edges.is_empty() && edges.iter().all(|e| e.as_str().unwrap().starts_with(['+', '-'])));
    assert!(d["waypoints"][0]["lat"].is_number() && d["waypoints"][0]["lon"].is_number(), "puntos con campos con nombre");
}

#[test]
fn geojson_uses_lon_lat_order() {
    let g = graph();
    let gj = graph_geojson(&g);
    let first = &gj["features"][0]["geometry"]["coordinates"][0];
    assert!(close(first[0].as_f64().unwrap(), LON0, 0.01), "la primera coordenada debe ser la longitud");
    let r = route(&g, &[geo(0.0, 0.0), geo(200.0, 0.0)], RouteOptions::default()).unwrap();
    let rj = route_geojson(&r);
    assert!(close(rj["features"][0]["geometry"]["coordinates"][0][1].as_f64().unwrap(), LAT0, 0.01));
}

// ── Giros ──

/// Una rampa que sale de la avenida en ángulo agudo. La ruta más corta pura pasa de largo
/// el empalme y entra con una media vuelta de ~146°; con el límite de giro tiene que
/// llegar por el otro lado, aunque sea mucho más largo.
fn ramp_graph() -> RoadGraph {
    let f = LocalFrame::new(GeoPoint::new(LAT0, LON0));
    let pts = [
        (1, 0.0, 0.0),
        (2, 100.0, 0.0),   // J: empalme de la rampa
        (3, 300.0, 0.0),
        (4, 600.0, 0.0),   // F
        (5, 250.0, 100.0), // R: final de la rampa
        (6, 600.0, 300.0), // G
        (7, 250.0, 300.0), // N
    ];
    let mut els: Vec<String> = pts
        .iter()
        .map(|(id, e, n)| {
            let p = f.en_to_geo([*e, *n]);
            format!(r#"{{"type":"node","id":{id},"lat":{},"lon":{}}}"#, p.lat, p.lon)
        })
        .collect();
    for (id, nodes) in [(10, "[1,2,3,4]"), (11, "[2,5]"), (12, "[4,6]"), (13, "[6,7]"), (14, "[7,5]")] {
        els.push(format!(r#"{{"type":"way","id":{id},"nodes":{nodes},"tags":{{"highway":"residential"}}}}"#));
    }
    let json = format!(r#"{{"elements":[{}]}}"#, els.join(","));
    let ex = Extract::from_overpass_json(&json).unwrap();
    RoadGraph::build(&ex, &BuildOptions { region: "rampa".into(), origin: Some(GeoPoint::new(LAT0, LON0)) }).unwrap()
}

#[test]
fn ramp_hairpin_is_avoided() {
    let g = ramp_graph();
    let f = g.frame;
    let from = f.en_to_geo([200.0, 0.0]); // sobre la avenida, pasado el empalme
    let to = f.en_to_geo([220.0, 80.0]); // sobre la rampa, cerca de su final
    let pure = route(&g, &[from, to], RouteOptions::shortest()).unwrap();
    let max_pure = pure.junction_turns(&g).into_iter().fold(0.0, f64::max);
    assert!(max_pure > 140.0, "el caso debería forzar una media vuelta: {max_pure:.0}°");
    assert!(pure.length_m < 300.0);

    let r = route(&g, &[from, to], RouteOptions::default()).unwrap();
    let max = r.junction_turns(&g).into_iter().fold(0.0, f64::max);
    assert!(max <= 135.0, "giro de {max:.0}° con límite de 135°");
    assert!(r.length_m > 1000.0, "debería dar la vuelta por el otro lado: {:.0} m", r.length_m);
}

#[test]
fn no_u_turn_at_an_intermediate_waypoint() {
    let g = graph();
    // Ir de oeste a este por la calle central y volver hacia el oeste desde un punto
    // de paso: sin media vuelta, el retorno tiene que rodear una manzana.
    let r = route(&g, &[geo(20.0, 0.0), geo(150.0, 0.0), geo(40.0, 0.0)], RouteOptions::default());
    match r {
        Ok(r) => assert!(r.length_m > 2.0 * 130.0 + 1.0, "volvió por la misma calle: {:.0} m", r.length_m),
        Err(e) => assert!(e.contains("giros"), "{e}"),
    }
}

#[test]
fn turn_angle_is_symmetric_and_bounded() {
    use track_compiler_core::routing::turn_angle_deg;
    assert!((turn_angle_deg([1.0, 0.0], [1.0, 0.0])).abs() < 1e-9);
    assert!((turn_angle_deg([1.0, 0.0], [0.0, 1.0]) - 90.0).abs() < 1e-9);
    assert!((turn_angle_deg([1.0, 0.0], [0.0, -1.0]) - 90.0).abs() < 1e-9);
    assert!((turn_angle_deg([1.0, 0.0], [-1.0, 0.0]) - 180.0).abs() < 1e-9);
}
