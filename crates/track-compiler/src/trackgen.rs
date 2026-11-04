//! Recorrido resuelto + elevación → pista para sim-core (S2).
//!
//! La salida es un `TrackDef` de sim-core (los mismos campos que `assets/tracks/*.json`)
//! más metadatos de procedencia, estado y diagnósticos, que sim-core ignora al leer.
//!
//! Espacios (documento de arquitectura v0.2): el plano local es este/norte en metros;
//! el motor usa X = este, Y = arriba, Z = −norte. La conversión de ejes ocurre aquí, una
//! sola vez, y las coordenadas se centran en el inicio de la pista para que sigan siendo
//! precisas en `f32`.

use crate::centerline::{self, CenterSample, CenterlineOptions};
use crate::dem::Dem;
use crate::diagnostics::{Diagnostic, Severity};
use crate::elevation::{self, ElevationOptions, Profile};
use crate::graph::RoadGraph;
use crate::routing::ResolvedRoute;
use serde_json::{json, Value};

#[derive(Clone, Debug)]
pub struct TrackOptions {
    pub id: String,
    pub name: String,
    /// Separación de los puntos de control que se exportan (sim-core interpola entre ellos).
    pub control_every_m: f64,
    /// Checkpoints intermedios como fracción del tramo.
    pub checkpoints: Vec<f64>,
    pub centerline: CenterlineOptions,
    pub elevation: ElevationOptions,
}

impl TrackOptions {
    pub fn new(id: &str, name: &str) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            control_every_m: 6.0,
            checkpoints: vec![0.33, 0.66],
            centerline: CenterlineOptions::default(),
            elevation: ElevationOptions::default(),
        }
    }
}

/// Estado de una pista generada (documento v0.2, diagnósticos del compilador).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Valid,
    Degraded,
    Unsupported,
}

impl Status {
    pub fn as_str(self) -> &'static str {
        match self {
            Status::Valid => "VALID",
            Status::Degraded => "DEGRADED",
            Status::Unsupported => "UNSUPPORTED",
        }
    }
}

pub struct GeneratedTrack {
    pub json: Value,
    pub samples: Vec<CenterSample>,
    pub profile: Profile,
    pub diagnostics: Vec<Diagnostic>,
    pub status: Status,
}

fn round(v: f64, digits: i32) -> f64 {
    let k = 10f64.powi(digits);
    (v * k).round() / k
}

pub fn generate(g: &RoadGraph, r: &ResolvedRoute, dem: &Dem, opts: &TrackOptions) -> Result<GeneratedTrack, String> {
    let samples = centerline::build(g, r, &opts.centerline)?;
    let profile = elevation::profile(&samples, dem, &g.frame, &opts.elevation)?;
    let mut diags = profile.diagnostics.clone();

    // Cruces atravesados: en S2 la pista es una sola cinta; las calles laterales y la
    // geometría de cada cruce llegan con S3 (Junction Compiler).
    let mut degree = vec![0u32; g.vertices.len()];
    for e in &g.edges {
        degree[e.from] += 1;
        degree[e.to] += 1;
    }
    let index: std::collections::BTreeMap<&str, usize> = g.edges.iter().enumerate().map(|(i, e)| (e.id.as_str(), i)).collect();
    let steps: Vec<(usize, bool)> = r
        .legs
        .iter()
        .flat_map(|l| l.steps.iter())
        .filter_map(|s| index.get(&s.0[1..]).map(|&i| (i, s.0.starts_with('+'))))
        .collect();
    let junctions = steps
        .windows(2)
        .filter(|w| {
            let (e, fwd) = (&g.edges[w[0].0], w[0].1);
            degree[if fwd { e.to } else { e.from }] >= 3
        })
        .count();
    // Contra el sentido: en una vía de calzadas separadas suele significar que el recorrido
    // va por la calzada del otro lado, que puede ir a otra altura (Costa Verde, km 2).
    let wrong_way: f64 =
        steps.iter().filter(|(i, fwd)| !g.edges[*i].oneway.allows(*fwd)).map(|(i, _)| g.edges[*i].length_m).sum();
    if wrong_way > 0.0 {
        diags.push(Diagnostic::new(
            Severity::Warning,
            "against-oneway",
            format!(
                "{wrong_way:.0} m del recorrido van contra el sentido de calles de un sentido; si la vía tiene \
                 calzadas separadas, probablemente vas por la del otro lado (usa --respect-oneway)"
            ),
        ));
    }
    if junctions > 0 {
        diags.push(Diagnostic::new(
            Severity::Info,
            "junctions-not-modeled",
            format!("la pista atraviesa {junctions} cruces; sus calles laterales no se modelan todavía (S3)"),
        ));
    }
    let estimated = samples.iter().filter(|c| g.edges[c.edge].width_estimated).count();
    if estimated > 0 {
        diags.push(Diagnostic::new(
            Severity::Info,
            "width-estimated",
            format!(
                "el ancho de {:.0} % de la pista se estimó por carriles y tipo de vía (acotado entre {:.0} y {:.0} m)",
                100.0 * estimated as f64 / samples.len() as f64,
                opts.centerline.min_width_m,
                opts.centerline.max_width_m
            ),
        ));
    }
    let turn_rate = centerline::max_turn_rate_deg_per_m(&samples);

    let status = if diags.iter().any(|d| d.severity == Severity::Error) {
        Status::Unsupported
    } else if diags.iter().any(|d| d.severity == Severity::Warning) {
        Status::Degraded
    } else {
        Status::Valid
    };

    // Puntos de control en el espacio del motor, centrados en el inicio.
    let o = samples[0].en;
    let every = ((opts.control_every_m / opts.centerline.spacing_m).round() as usize).max(1);
    let mut picks: Vec<usize> = (0..samples.len()).step_by(every).collect();
    if *picks.last().unwrap() != samples.len() - 1 {
        picks.push(samples.len() - 1);
    }
    let points: Vec<Value> = picks
        .iter()
        .map(|&i| {
            let c = &samples[i];
            json!({
                "x": round(c.en[0] - o[0], 3),
                "z": round(-(c.en[1] - o[1]), 3),
                "y": round(profile.filtered[i], 3),
                "width": round(c.width, 2),
            })
        })
        .collect();

    let origin = g.frame.en_to_geo(o);
    let (min_h, max_h) = profile.filtered.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), &h| (a.min(h), b.max(h)));
    let length = samples.last().unwrap().s;
    let json = json!({
        "id": opts.id,
        "name": opts.name,
        "closed": r.closed,
        "sampleSpacing": opts.centerline.spacing_m,
        "defaultWidth": 12.0,
        "checkpoints": opts.checkpoints,
        "points": points,
        "status": status.as_str(),
        "diagnostics": diags,
        "stats": {
            "lengthM": round(length, 1),
            "climbM": round(profile.climb_m, 1),
            "maxGradePct": round(profile.max_grade * 100.0, 1),
            "minElevationM": round(min_h, 1),
            "maxElevationM": round(max_h, 1),
            "maxTurnRateDegPerM": round(turn_rate, 2),
        },
        "source": {
            "generator": format!("track-compiler {}", crate::COMPILER_VERSION),
            "graph": format!("{}@{}", g.region, g.hash),
            "osmBaseTimestamp": g.source.osm_base_timestamp,
            "dem": { "product": "Copernicus GLO-30", "hash": dem.hash },
            "origin": { "lat": round(origin.lat, 7), "lon": round(origin.lon, 7) },
            "axes": "x = este, y = altura (m), z = -norte; metros desde el origen",
            "route": r.descriptor_route(),
            "elevationAnchors": opts
                .elevation
                .anchors
                .iter()
                .map(|a| json!({ "lat": a.at.lat, "lon": a.at.lon, "heightM": a.height_m }))
                .collect::<Vec<_>>(),
            "attribution": "© colaboradores de OpenStreetMap (ODbL) · Copernicus DEM GLO-30 © DLR e.V. 2010-2014 y © Airbus Defence and Space GmbH 2014-2018, provisto bajo COPERNICUS por la Unión Europea y la ESA",
        },
    });
    Ok(GeneratedTrack { json, samples, profile, diagnostics: diags, status })
}

/// Perfil en CSV para revisarlo en una planilla: s, altura cruda, filtrada, ancho, estructura.
pub fn profile_csv(t: &GeneratedTrack) -> String {
    let mut out = String::from("s_m,elevacion_eje_m,elevacion_a_lo_ancho_m,elevacion_pista_m,variacion_lateral_m,ancho_m,estructura\n");
    let opt = |v: Option<f64>| v.map_or(String::new(), |h| format!("{h:.2}"));
    for (i, c) in t.samples.iter().enumerate() {
        let kind = if c.bridge { "puente" } else if c.tunnel { "tunel" } else { "" };
        out.push_str(&format!(
            "{:.1},{},{},{:.2},{:.1},{:.2},{}\n",
            c.s,
            opt(t.profile.center[i]),
            opt(t.profile.raw[i]),
            t.profile.filtered[i],
            t.profile.spread[i],
            c.width,
            kind
        ));
    }
    out
}
