//! Depuración: aristas que tocan unos vértices del grafo, con la dirección en que salen.
//! Uso: --example vertex -- <grafo.json> <vértice> [<vértice> ...]
use track_compiler_core::graph::RoadGraph;
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let g = RoadGraph::from_json(&std::fs::read_to_string(&a[1]).unwrap()).unwrap();
    for v in a[2..].iter().map(|s| s.parse::<usize>().unwrap()) {
        let p = g.frame.en_to_geo(g.vertices[v].en);
        println!("v{v} {:.6},{:.6}", p.lat, p.lon);
        for e in g.edges.iter().filter(|e| e.from == v || e.to == v) {
            let out = e.from == v;
            let (a, b) = if out { (e.en[0], e.en[1]) } else { (e.en[e.en.len() - 1], e.en[e.en.len() - 2]) };
            let bearing = (b[0] - a[0]).atan2(b[1] - a[1]).to_degrees().rem_euclid(360.0);
            let other = if out { e.to } else { e.from };
            println!(
                "   {:>16} {:<28} sale a {:>3.0}° hacia v{} · {:.0} m · {:?}{}",
                e.id, e.name.as_deref().unwrap_or("(sin nombre)"), bearing, other, e.length_m, e.oneway,
                if out { "" } else { " (llega)" }
            );
        }
    }
}
