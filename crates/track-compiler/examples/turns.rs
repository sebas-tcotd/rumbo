//! Depuración: lista los giros de más de 100° de la ruta más corta pura entre puntos.
//! Uso: cargo run -p track-compiler --release --example turns -- <grafo.json> lat,lon lat,lon [...]
use track_compiler::graph::RoadGraph;
use track_compiler::routing::{route, RouteOptions};
use track_compiler::GeoPoint;
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let g = RoadGraph::from_json(&std::fs::read_to_string(&a[1]).unwrap()).unwrap();
    let pts: Vec<GeoPoint> = a[2..].iter().map(|s| GeoPoint::parse(s).unwrap()).collect();
    let r = route(&g, &pts, RouteOptions::shortest()).unwrap();
    let steps: Vec<&str> = r.legs.iter().flat_map(|l| l.steps.iter().map(|s| s.0.as_str())).collect();
    for (i, t) in r.junction_turns(&g).iter().enumerate() {
        if *t > 100.0 {
            let id = &steps[i + 1][1..];
            let e = g.edges.iter().find(|e| e.id == id).unwrap();
            let v = &g.vertices[if steps[i + 1].starts_with('+') { e.from } else { e.to }];
            println!("{:.0}° entre {} y {} en {:.6},{:.6} ({:?}, {:?})", t, steps[i], steps[i + 1], v.lat, v.lon, e.name, e.oneway);
        }
    }
    println!("pasos: {} … {}", steps[0], steps[steps.len() - 1]);
}
