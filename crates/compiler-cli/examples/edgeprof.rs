//! Depuración: geometría y altura del DEM a lo largo de unas aristas.
//! Uso: --example edgeprof -- <grafo.json> <dem.tif> <id> [<id> ...]
use track_compiler_core::graph::RoadGraph;
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let g = RoadGraph::from_json(&std::fs::read_to_string(&a[1]).unwrap()).unwrap();
    let dem = compiler_cli::adapters::geotiff::from_path(&a[2]).unwrap();
    for id in &a[3..] {
        let e = g.edges.iter().find(|e| &e.id == id).unwrap();
        let p0 = g.frame.en_to_geo(e.en[0]);
        let p1 = g.frame.en_to_geo(*e.en.last().unwrap());
        print!(
            "{id:>16} {:.5},{:.5} -> {:.5},{:.5} | ",
            p0.lat, p0.lon, p1.lat, p1.lon
        );
        let n = e.en.len();
        for k in 0..=4 {
            let t = k as f64 / 4.0 * (n - 1) as f64;
            let i = (t.floor() as usize).min(n - 2);
            let f = t - i as f64;
            let p = [
                e.en[i][0] + (e.en[i + 1][0] - e.en[i][0]) * f,
                e.en[i][1] + (e.en[i + 1][1] - e.en[i][1]) * f,
            ];
            let q = g.frame.en_to_geo(p);
            print!(" {:.1}", dem.height(q).unwrap_or(f64::NAN));
            if k == 2 {
                print!(" [mitad {:.6},{:.6}]", q.lat, q.lon);
            }
        }
        println!();
    }
}
