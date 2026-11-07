//! Depuración: aristas cuyo nombre contiene un texto, con extremos, grado de los vértices y atributos.
//! Uso: --example streets -- <grafo.json> <texto> [<texto> ...]
use track_compiler_core::graph::RoadGraph;
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let g = RoadGraph::from_json(&std::fs::read_to_string(&a[1]).unwrap()).unwrap();
    let mut degree = vec![0usize; g.vertices.len()];
    for e in &g.edges {
        degree[e.from] += 1;
        degree[e.to] += 1;
    }
    for q in &a[2..] {
        let q = q.to_lowercase();
        for e in g.edges.iter().filter(|e| e.name.as_deref().unwrap_or("").to_lowercase().contains(&q)) {
            let p0 = g.frame.en_to_geo(e.en[0]);
            let p1 = g.frame.en_to_geo(*e.en.last().unwrap());
            println!(
                "{:>16} {:?} {:?} v{}(g{}) {:.6},{:.6} -> v{}(g{}) {:.6},{:.6} · {:.0} m · {:?} · ancho {:.1} m{} · carriles {}",
                e.id, e.name.as_deref().unwrap_or(""), e.class, e.from, degree[e.from], p0.lat, p0.lon, e.to, degree[e.to], p1.lat, p1.lon,
                e.length_m, e.oneway, e.width_m, if e.width_estimated { " (estimado)" } else { "" }, e.lanes
            );
        }
    }
}
