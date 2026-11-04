//! CLI de S1.
//!
//!   track-compiler ingest <overpass.json> --region <id> --out <grafo.json> [--geojson <archivo>] [--origin lat,lon]
//!   track-compiler route  <grafo.json> --from lat,lon --to lat,lon [--via lat,lon]... [--closed]
//!                         [--respect-oneway] [--max-turn <grados>] [--turn-cost <metros>]
//!                         [--geojson <archivo>] [--descriptor <archivo>]
//!   track-compiler stats  <grafo.json>

use std::process::ExitCode;
use track_compiler::export::{graph_geojson, route_geojson};
use track_compiler::graph::{BuildOptions, RoadGraph};
use track_compiler::osm::Extract;
use track_compiler::routing::{route, RouteOptions};
use track_compiler::{GeoPoint, Severity};

const USAGE: &str = "uso:
  track-compiler ingest <overpass.json> --region <id> --out <grafo.json> [--geojson <archivo>] [--origin lat,lon]
  track-compiler route  <grafo.json> --from lat,lon --to lat,lon [--via lat,lon]... [--closed] [--respect-oneway]
                        [--max-turn <grados, 135>] [--turn-cost <metros por giro de 90°, 20>]
                        [--geojson <archivo>] [--descriptor <archivo>]
  track-compiler stats  <grafo.json>";

struct Args {
    positional: Vec<String>,
    flags: Vec<(String, Option<String>)>,
}

impl Args {
    fn parse(raw: &[String]) -> Self {
        let mut positional = Vec::new();
        let mut flags = Vec::new();
        let mut i = 0;
        while i < raw.len() {
            let a = &raw[i];
            if let Some(name) = a.strip_prefix("--") {
                let takes_value = !matches!(name, "closed" | "respect-oneway");
                let value = if takes_value { raw.get(i + 1).cloned() } else { None };
                if takes_value {
                    i += 1;
                }
                flags.push((name.to_string(), value));
            } else {
                positional.push(a.clone());
            }
            i += 1;
        }
        Self { positional, flags }
    }
    fn get(&self, k: &str) -> Option<&str> {
        self.flags.iter().find(|(n, _)| n == k).and_then(|(_, v)| v.as_deref())
    }
    fn all(&self, k: &str) -> Vec<&str> {
        self.flags.iter().filter(|(n, _)| n == k).filter_map(|(_, v)| v.as_deref()).collect()
    }
    fn has(&self, k: &str) -> bool {
        self.flags.iter().any(|(n, _)| n == k)
    }
}

fn read(path: &str) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|e| format!("no se pudo leer {path}: {e}"))
}

fn write(path: &str, v: &serde_json::Value, pretty: bool) -> Result<(), String> {
    if let Some(dir) = std::path::Path::new(path).parent() {
        if !dir.as_os_str().is_empty() {
            std::fs::create_dir_all(dir).map_err(|e| format!("no se pudo crear {}: {e}", dir.display()))?;
        }
    }
    let s = if pretty { serde_json::to_string_pretty(v) } else { serde_json::to_string(v) }.map_err(|e| e.to_string())?;
    std::fs::write(path, s).map_err(|e| format!("no se pudo escribir {path}: {e}"))
}

fn print_diagnostics(g: &RoadGraph) {
    for d in &g.diagnostics {
        let tag = match d.severity {
            Severity::Info => "info",
            Severity::Warning => "aviso",
            Severity::Error => "error",
        };
        let way = d.way.map(|w| format!(" (vía {w})")).unwrap_or_default();
        println!("  [{tag}] {}{way}", d.message);
    }
}

fn stats(g: &RoadGraph) {
    let km = g.total_length_km();
    let oneway = g.edges.iter().filter(|e| e.oneway != track_compiler::graph::Oneway::Both).count();
    let bridges = g.edges.iter().filter(|e| e.bridge).count();
    let tunnels = g.edges.iter().filter(|e| e.tunnel).count();
    println!("región {} · grafo {} · compilador {}", g.region, g.hash, g.compiler_version);
    println!("origen del marco local: {:.5},{:.5}", g.frame.origin.lat, g.frame.origin.lon);
    if let Some(t) = &g.source.osm_base_timestamp {
        println!("datos de OSM al {t} · {}", g.source.attribution);
    }
    println!(
        "{} nodos · {} aristas · {:.1} km de calle · {} de un sentido · {} en puentes · {} en túneles",
        g.vertices.len(),
        g.edges.len(),
        km,
        oneway,
        bridges,
        tunnels
    );
    println!("componentes: {} (el mayor con {} nodos)", g.components.count, g.components.largest_vertices);
    print_diagnostics(g);
}

fn run(raw: Vec<String>) -> Result<(), String> {
    let args = Args::parse(&raw);
    let cmd = args.positional.first().map(String::as_str).ok_or(USAGE)?;
    match cmd {
        "ingest" => {
            let input = args.positional.get(1).ok_or(USAGE)?;
            let region = args.get("region").ok_or("falta --region")?;
            let out = args.get("out").ok_or("falta --out")?;
            let origin = args.get("origin").map(GeoPoint::parse).transpose()?;
            let ex = Extract::from_overpass_json(&read(input)?)?;
            let g = RoadGraph::build(&ex, &BuildOptions { region: region.into(), origin })?;
            write(out, &serde_json::to_value(&g).map_err(|e| e.to_string())?, false)?;
            stats(&g);
            println!("grafo escrito en {out}");
            if let Some(path) = args.get("geojson") {
                write(path, &graph_geojson(&g), false)?;
                println!("GeoJSON del grafo en {path} (arrástralo a geojson.io para revisarlo sobre el mapa)");
            }
            Ok(())
        }
        "route" => {
            let input = args.positional.get(1).ok_or(USAGE)?;
            let g = RoadGraph::from_json(&read(input)?)?;
            let from = GeoPoint::parse(args.get("from").ok_or("falta --from")?)?;
            let to = GeoPoint::parse(args.get("to").ok_or("falta --to")?)?;
            let mut pts = vec![from];
            for v in args.all("via") {
                pts.push(GeoPoint::parse(v)?);
            }
            pts.push(to);
            let num = |k: &str, d: f64| -> Result<f64, String> {
                args.get(k).map_or(Ok(d), |v| v.parse::<f64>().map_err(|_| format!("--{k} debe ser un número")))
            };
            let base = RouteOptions::default();
            let opts = RouteOptions {
                respect_oneway: args.has("respect-oneway"),
                closed: args.has("closed"),
                max_turn_deg: num("max-turn", base.max_turn_deg)?,
                turn_cost_m: num("turn-cost", base.turn_cost_m)?,
            };
            let r = route(&g, &pts, opts)?;
            println!(
                "ruta de {:.0} m en {} tramos ({}){}",
                r.length_m,
                r.legs.iter().map(|l| l.steps.len()).sum::<usize>(),
                if opts.respect_oneway { "respetando sentidos" } else { "sin respetar sentidos" },
                if opts.closed { ", circuito cerrado" } else { "" }
            );
            let turns = r.junction_turns(&g);
            let sharp = turns.iter().filter(|t| **t > 90.0).count();
            println!(
                "  giro más cerrado en un cruce: {:.0}° (máximo permitido {:.0}°) · {} giros de más de 90°",
                turns.iter().copied().fold(0.0, f64::max),
                opts.max_turn_deg,
                sharp
            );
            for (i, l) in r.legs.iter().enumerate() {
                println!(
                    "  tramo {}: {:.0} m · punto ajustado a {:.1} m de lo pedido, sobre {}",
                    i + 1,
                    l.length_m,
                    l.from.offset_m,
                    l.from.edge
                );
            }
            if let Some(path) = args.get("geojson") {
                write(path, &route_geojson(&r), false)?;
                println!("GeoJSON de la ruta en {path}");
            }
            let descriptor = r.descriptor_route();
            if let Some(path) = args.get("descriptor") {
                write(path, &descriptor, true)?;
                println!("fragmento de descriptor en {path}");
            }
            Ok(())
        }
        "stats" => {
            let input = args.positional.get(1).ok_or(USAGE)?;
            stats(&RoadGraph::from_json(&read(input)?)?);
            Ok(())
        }
        _ => Err(USAGE.into()),
    }
}

fn main() -> ExitCode {
    match run(std::env::args().skip(1).collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}
