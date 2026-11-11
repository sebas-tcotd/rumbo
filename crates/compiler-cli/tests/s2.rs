//! Pruebas de S2: modelo de elevación, perfil filtrado, eje y generación de la pista.
#![allow(clippy::unwrap_used, clippy::expect_used, reason = "test code: a panic is the failure report")]
use track_compiler_core::dem::Dem;
use track_compiler_core::elevation::{filter, Anchor, ElevationOptions};
use track_compiler_core::graph::{BuildOptions, RoadGraph};
use track_compiler_core::osm::Extract;
use track_compiler_core::routing::{route, RouteOptions};
use track_compiler_core::trackgen::{generate, TrackOptions};
use track_compiler_core::GeoPoint;

const DEM: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/raw/Copernicus_DSM_COG_10_S13_00_W078_00_DEM.tif");
const OSM: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/raw/lima-costa-verde-2026-11-04.overpass.json");
const ASSET: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/tracks/costa-verde-sprint.json");

fn have_data() -> bool {
    let ok = std::path::Path::new(DEM).exists() && std::path::Path::new(OSM).exists();
    if !ok {
        eprintln!("faltan los datos de data/raw: se omite");
    }
    ok
}

#[test]
fn dem_reads_known_heights() {
    if !have_data() {
        return;
    }
    let dem = compiler_cli::adapters::geotiff::from_path(DEM).unwrap();
    assert_eq!((dem.width, dem.height), (3600, 3600));
    let near = |lat, lon, h: f64, tol: f64| {
        let v = dem.height(GeoPoint::new(lat, lon)).unwrap();
        assert!((v - h).abs() < tol, "{lat},{lon}: {v:.1} m, esperaba ~{h} m");
    };
    near(-12.119523, -77.029105, 89.0, 3.0); // cerca del Parque Kennedy, arriba del acantilado
    near(-12.130823, -77.034169, 8.5, 3.0); // Circuito de Playas, al pie de la Bajada Balta
    assert!(dem.height(GeoPoint::new(-11.5, -77.0)).is_none(), "fuera del tile no hay altura");
}

#[test]
fn filter_rejects_short_spikes_and_limits_grade() {
    // Vía plana a 8 m con dos picos cortos (un edificio, el borde del acantilado) y un hueco.
    let ds = 2.0;
    let mut raw: Vec<Option<f64>> = (0..600).map(|_| Some(8.0)).collect();
    for h in &mut raw[200..215] {
        *h = Some(30.0);
    }
    for h in &mut raw[400..410] {
        *h = Some(-5.0);
    }
    for h in &mut raw[300..320] {
        *h = None; // puente: se interpola
    }
    let opts = ElevationOptions::default();
    let f = filter(&raw, ds, &opts).unwrap();
    assert!(f.iter().all(|h| (h - 8.0).abs() < 1.0), "el perfil debería quedar plano: {:?}", f.iter().fold((f64::MAX, f64::MIN), |a, h| (a.0.min(*h), a.1.max(*h))));
    // Escalón real de 30 m: el filtro lo convierte en una rampa dentro del límite de pendiente.
    let step: Vec<Option<f64>> = (0..800).map(|i| Some(if i < 400 { 5.0 } else { 35.0 })).collect();
    let g = filter(&step, ds, &opts).unwrap();
    let max_grade = g.windows(2).map(|w| ((w[1] - w[0]) / ds).abs()).fold(0.0, f64::max);
    assert!(max_grade <= opts.max_grade + 1e-9, "pendiente {max_grade}");
    assert!((g[0] - 5.0).abs() < 0.5 && (g[799] - 35.0).abs() < 0.5, "conserva los niveles de los extremos");
}

/// Receta de la pista de `assets/`: se lee de su propio bloque `source`, así la prueba
/// reconstruye exactamente lo que se generó (puntos, opciones de ruta y anclas).
fn recipe() -> (Vec<GeoPoint>, RouteOptions, TrackOptions) {
    let a: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(ASSET).unwrap()).unwrap();
    let src = &a["source"];
    let pts: Vec<GeoPoint> = serde_json::from_value(src["route"]["waypoints"].clone()).unwrap();
    let path = &src["route"]["resolvedPath"];
    let opts = RouteOptions {
        respect_oneway: path["respectOneway"].as_bool().unwrap(),
        closed: src["route"]["closed"].as_bool().unwrap(),
        max_turn_deg: path["maxTurnDeg"].as_f64().unwrap(),
        turn_cost_m: path["turnCostM"].as_f64().unwrap(),
    };
    let mut t = TrackOptions::new(a["id"].as_str().unwrap(), a["name"].as_str().unwrap());
    for x in src["elevationAnchors"].as_array().unwrap() {
        t.elevation.anchors.push(Anchor {
            at: GeoPoint::new(x["lat"].as_f64().unwrap(), x["lon"].as_f64().unwrap()),
            height_m: x["heightM"].as_f64().unwrap(),
        });
    }
    (pts, opts, t)
}

fn costa_verde() -> (RoadGraph, track_compiler_core::routing::ResolvedRoute, Dem, TrackOptions) {
    let ex = Extract::from_overpass_json(&std::fs::read_to_string(OSM).unwrap()).unwrap();
    let g = RoadGraph::build(&ex, &BuildOptions { region: "lima-costa-verde".into(), origin: None }).unwrap();
    let (pts, opts, t) = recipe();
    let r = route(&g, &pts, opts).unwrap();
    (g, r, compiler_cli::adapters::geotiff::from_path(DEM).unwrap(), t)
}

#[test]
fn generated_track_is_smooth_and_consistent() {
    if !have_data() {
        return;
    }
    let (g, r, dem, recipe) = costa_verde();
    let t = generate(&g, &r, &dem, &recipe).unwrap();
    assert!(t.diagnostics.iter().all(|d| d.code != "against-oneway"), "la pista va contra el sentido de alguna calle");
    let spacing: Vec<f64> = t.samples.windows(2).map(|w| w[1].s - w[0].s).collect();
    assert!(spacing.iter().all(|d| (d - 2.0).abs() < 0.6), "paso irregular");
    assert!((t.samples.last().unwrap().s - r.length_m).abs() / r.length_m < 0.02, "el suavizado acortó o alargó demasiado el recorrido");
    let rate = track_compiler_core::centerline::max_turn_rate_deg_per_m(&t.samples);
    assert!(rate < 6.0, "quiebre de {rate:.1}°/m en el eje");
    assert!(t.profile.max_grade <= 0.14 + 1e-9);
    assert!(t.samples.iter().all(|c| (7.0..=16.0).contains(&c.width)));
    // Una ancla corrige la altura donde se pone, la pendiente sigue acotada y el efecto es
    // local: más allá de su alcance la pista no cambia.
    let mut opts = TrackOptions::new("x", "x");
    let base = generate(&g, &r, &dem, &opts).unwrap();
    let at = g.frame.en_to_geo(t.samples[3000].en);
    let want = base.profile.filtered[3000] - 8.0;
    opts.elevation.anchors.push(Anchor { at, height_m: want });
    let a = generate(&g, &r, &dem, &opts).unwrap();
    assert!((a.profile.filtered[3000] - want).abs() < 0.5, "ancla no aplicada: {} en vez de {want}", a.profile.filtered[3000]);
    assert!(a.profile.max_grade <= 0.14 + 1e-9);
    let reach = (opts.elevation.anchor_reach_m / 2.0) as usize + 60; // + margen del suavizado y la pendiente
    for i in (0..3000 - reach).chain(3000 + reach..a.samples.len()) {
        assert!((a.profile.filtered[i] - base.profile.filtered[i]).abs() < 1e-9, "el ancla movió la muestra {i}");
    }
    // Dos anclas cercanas fijan el tramo entre ellas en línea recta, aunque el modelo tenga una loma.
    let mut opts = TrackOptions::new("x", "x");
    for (k, h) in [(2000usize, 5.0), (2150, 8.0)] {
        opts.elevation.anchors.push(Anchor { at: g.frame.en_to_geo(t.samples[k].en), height_m: h });
    }
    let b = generate(&g, &r, &dem, &opts).unwrap();
    for k in 2030..2120 {
        let want = 5.0 + 3.0 * (k - 2000) as f64 / 150.0;
        assert!((b.profile.filtered[k] - want).abs() < 0.3, "muestra {k}: {} en vez de {want}", b.profile.filtered[k]);
    }
}

/// La pista de `assets/` debe ser exactamente la que produce el compilador actual.
/// Si esto falla, regenérala (ver README, sección S2) y súbela junto con el cambio.
#[test]
fn committed_asset_is_up_to_date() {
    if !have_data() || !std::path::Path::new(ASSET).exists() {
        return;
    }
    let (g, r, dem, recipe) = costa_verde();
    let t = generate(&g, &r, &dem, &recipe).unwrap();
    let committed: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(ASSET).unwrap()).unwrap();
    let definition = sim_core::TrackDef::from_json(&t.json.to_string()).unwrap();
    assert_eq!(sim_core::compile_track(&definition).unwrap().hash, 0x438fcda7191f2309);
    assert_eq!(committed, t.json, "assets/tracks/costa-verde-sprint.json está desactualizada: regénerala");
}
