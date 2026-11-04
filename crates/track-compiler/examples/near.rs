//! Depuración: aristas cercanas a un punto, marcando las que usa una ruta.
//! Uso: cargo run -p track-compiler --release --example near -- <grafo.json> <punto lat,lon> <radio m> <desde lat,lon> <hasta lat,lon>
use track_compiler::graph::RoadGraph;
use track_compiler::routing::{route, RouteOptions};
use track_compiler::GeoPoint;
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let g = RoadGraph::from_json(&std::fs::read_to_string(&a[1]).unwrap()).unwrap();
    let p = g.frame.to_en(GeoPoint::parse(&a[2]).unwrap());
    let radius: f64 = a[3].parse().unwrap();
    let r = route(
        &g,
        &[
            GeoPoint::parse(&a[4]).unwrap(),
            GeoPoint::parse(&a[5]).unwrap(),
        ],
        RouteOptions::default(),
    )
    .unwrap();
    let used: std::collections::BTreeSet<String> = r
        .legs
        .iter()
        .flat_map(|l| l.steps.iter().map(|s| s.0[1..].to_string()))
        .collect();
    for e in &g.edges {
        let d =
            e.en.iter()
                .map(|q| ((q[0] - p[0]).powi(2) + (q[1] - p[1]).powi(2)).sqrt())
                .fold(f64::INFINITY, f64::min);
        if d <= radius {
            println!(
                "{} {:>10} {:>5.0} m de distancia · {:?} {:?} sentido={:?} capa={} puente={} · {:.0} m",
                if used.contains(&e.id) { "RUTA" } else { "    " },
                e.id, d, e.class, e.name, e.oneway, e.layer, e.bridge, e.length_m
            );
        }
    }
}
