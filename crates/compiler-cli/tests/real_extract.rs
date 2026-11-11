//! Pruebas sobre un extracto real de OSM (criterio de salida de S1).
//!
//! Se activan solas si hay un archivo `data/raw/*.overpass.json` (el más reciente) o si
//! `RUMBO_OSM` apunta a uno. Sin datos, avisan y pasan: los datos no viajan en CI.
//! Descarga: `node tools/data-fetch/fetch-osm.mjs`.
#![allow(clippy::unwrap_used, clippy::expect_used, reason = "test code: a panic is the failure report")]
use std::path::PathBuf;
use track_compiler_core::graph::{BuildOptions, RoadGraph};
use track_compiler_core::osm::Extract;
use track_compiler_core::routing::{route, RouteOptions};
use track_compiler_core::GeoPoint;

fn extract_path() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("RUMBO_OSM") {
        return Some(PathBuf::from(p));
    }
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data/raw");
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.to_string_lossy().ends_with(".overpass.json"))
        .collect();
    files.sort();
    files.pop()
}

fn load() -> Option<(RoadGraph, String)> {
    let Some(path) = extract_path() else {
        eprintln!("sin extracto real en data/raw: se omite (descárgalo con `node tools/data-fetch/fetch-osm.mjs`)");
        return None;
    };
    let text = std::fs::read_to_string(&path).expect("no se pudo leer el extracto");
    let ex = Extract::from_overpass_json(&text).expect("extracto inválido");
    let g = RoadGraph::build(&ex, &BuildOptions { region: "real".into(), origin: None }).expect("no se pudo construir el grafo");
    Some((g, text))
}

#[test]
fn real_graph_is_mostly_connected_and_stable() {
    let Some((g, text)) = load() else { return };
    let share = g.components.largest_vertices as f64 / g.vertices.len() as f64;
    eprintln!(
        "{} vértices, {} aristas, {} componentes, la mayor con {:.1} %",
        g.vertices.len(),
        g.edges.len(),
        g.components.count,
        share * 100.0
    );
    assert!(g.edges.len() > 500, "muy pocas aristas: ¿la caja de la consulta es la correcta?");
    assert!(share > 0.85, "la red está demasiado fragmentada ({:.1} % en la mayor componente)", share * 100.0);
    // Mismo extracto, mismo hash.
    let ex = Extract::from_overpass_json(&text).unwrap();
    let again = RoadGraph::build(&ex, &BuildOptions { region: "real".into(), origin: None }).unwrap();
    assert_eq!(g.hash, again.hash);
}

/// Rutas entre vértices lejanos de la componente principal: deben existir, ser
/// deterministas y tener una longitud razonable frente a la distancia en línea recta.
#[test]
fn real_routes_are_valid_and_deterministic() {
    let Some((g, _)) = load() else { return };
    let mut count = std::collections::BTreeMap::<u32, usize>::new();
    for c in &g.component {
        *count.entry(*c).or_default() += 1;
    }
    let main = count.iter().max_by_key(|(c, n)| (**n, std::cmp::Reverse(**c))).map(|(c, _)| *c).unwrap();
    let pool: Vec<_> = g.vertices.iter().zip(&g.component).filter(|(_, c)| **c == main).map(|(v, _)| v).collect();
    let n = pool.len();
    let mut checked = 0;
    for k in 0..12 {
        let a = pool[(k * 7919) % n];
        let b = pool[(k * 104_729 + n / 2) % n];
        let straight = ((a.en[0] - b.en[0]).powi(2) + (a.en[1] - b.en[1]).powi(2)).sqrt();
        if straight < 1500.0 {
            continue;
        }
        let pts = [GeoPoint::new(a.lat, a.lon), GeoPoint::new(b.lat, b.lon)];
        let r1 = route(&g, &pts, RouteOptions::default()).expect("ruta entre vértices conectados");
        let r2 = route(&g, &pts, RouteOptions::default()).unwrap();
        assert_eq!(r1.descriptor_route(), r2.descriptor_route(), "la ruta no es determinista");
        let ratio = r1.length_m / straight;
        assert!((1.0..3.0).contains(&ratio), "ruta de {:.0} m para {:.0} m en línea recta", r1.length_m, straight);
        // Con la ruta más corta pura: respetando sentidos puede no existir, pero si existe
        // no es más corta.
        let pure = route(&g, &pts, RouteOptions::shortest()).unwrap();
        if let Ok(r3) = route(&g, &pts, RouteOptions { respect_oneway: true, ..RouteOptions::shortest() }) {
            assert!(r3.length_m + 1e-6 >= pure.length_m);
        }
        // El límite de giro solo puede alargar la ruta, nunca acortarla.
        assert!(r1.length_m + 1e-6 >= pure.length_m);
        checked += 1;
    }
    eprintln!("{checked} rutas verificadas");
    assert!(checked >= 4, "no hubo suficientes pares lejanos para probar");
}
