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
use crate::ports::ElevationSource;
use crate::diagnostics::{Diagnostic, Severity};
use crate::elevation::{self, ElevationOptions, Profile};
use crate::graph::RoadGraph;
use crate::network::{self, NetworkOptions};
use crate::routing::ResolvedRoute;
use crate::terrain::{HeightField, TerrainOptions};
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
    /// S3: si está, la pista lleva su red conducible (calles, cruces y calles laterales) y
    /// la altura sale de un campo 2D (`terrain`) en lugar del perfil a lo largo del eje.
    pub network: Option<NetworkOptions>,
    pub terrain: TerrainOptions,
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
            network: None,
            terrain: TerrainOptions::default(),
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

pub fn generate(g: &RoadGraph, r: &ResolvedRoute, dem: &dyn ElevationSource, opts: &TrackOptions) -> Result<GeneratedTrack, String> {
    let mut cl = opts.centerline;
    if let Some(n) = &opts.network {
        cl.min_width_m = n.min_width_m;
        cl.max_width_m = n.max_width_m;
    }
    // En una red, el eje sale de la línea de referencia (arcos en los giros).
    let net = opts.network.as_ref().map(|n| network::build(g, r, n)).transpose()?;
    let samples = match &net {
        None => centerline::build(g, r, &cl)?,
        Some(net) => {
            let step = ((cl.spacing_m).round() as usize).max(1);
            let mut s = 0.0;
            let mut prev: Option<[f64; 2]> = None;
            net.reference
                .iter()
                .step_by(step)
                .map(|q| {
                    if let Some(p) = prev {
                        s += libm::hypot(q.en[0] - p[0], q.en[1] - p[1]);
                    }
                    prev = Some(q.en);
                    CenterSample { en: q.en, s, edge: q.edge, width: q.width, bridge: false, tunnel: false }
                })
                .collect()
        }
    };
    let mut profile = elevation::profile(&samples, dem, &g.frame, &opts.elevation)?;
    let mut diags = profile.diagnostics.clone();

    // S3: red conducible y altura como campo 2D.
    let mut network_json = Value::Null;
    let mut junctions_json = Value::Null;
    if let Some(net) = &net {
        let (mut mn, mut mx) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
        for p in net.vertices.iter().chain(samples.iter().map(|c| &c.en)) {
            mn = [mn[0].min(p[0]), mn[1].min(p[1])];
            mx = [mx[0].max(p[0]), mx[1].max(p[1])];
        }
        let field = HeightField::build(dem, &g.frame, mn, mx, &opts.terrain)?;
        profile.filtered = samples.iter().map(|c| field.at(c.en)).collect();
        let ds = cl.spacing_m;
        profile.climb_m = profile.filtered.windows(2).map(|w| (w[1] - w[0]).max(0.0)).sum();
        profile.max_grade = profile.filtered.windows(2).map(|w| ((w[1] - w[0]) / ds).abs()).fold(0.0, f64::max);
        // Los avisos del perfil 1D (paredes, altura estimada) no aplican: en la ciudad las
        // fachadas siempre "varían a lo ancho". La altura sale del campo.
        diags.retain(|d| !d.code.starts_with("elevation-"));
        diags.push(Diagnostic::new(
            Severity::Info,
            "elevation-field",
            format!(
                "altura de la red: percentil {:.0} del modelo en {:.0} m a la redonda, suavizado de {:.0} m",
                opts.terrain.percentile, opts.terrain.radius_m, opts.terrain.sigma_m
            ),
        ));
        diags.extend(net.diagnostics.iter().cloned());
        for j in &net.junctions {
            let (sev, code) = match j.status {
                Status::Valid => continue,
                Status::Degraded => (Severity::Warning, "junction-degraded"),
                Status::Unsupported => (Severity::Error, "junction-unsupported"),
            };
            diags.push(
                Diagnostic::new(sev, code, format!("{} de {} brazos: {}", j.kind, j.arms, j.reason.clone().unwrap_or_default()))
                    .at(g.frame.en_to_geo(j.en)),
            );
        }
        let o = samples[0].en;
        let xyz = |p: [f64; 2]| json!([round(p[0] - o[0], 3), round(field.at(p), 3), round(-(p[1] - o[1]), 3)]);
        network_json = json!({
            "vertices": net.vertices.iter().map(|&p| xyz(p)).collect::<Vec<_>>(),
            "triangles": net.triangles.iter().map(|(t, _)| json!(t)).collect::<Vec<_>>(),
            "surfaces": net.triangles.iter().map(|(_, s)| *s).collect::<Vec<_>>(),
            "walls": net.walls.iter().map(|w| w.iter().map(|&p| xyz(p)).collect::<Vec<_>>()).collect::<Vec<_>>(),
            "barriers": net.barriers.iter().map(|b| json!({
                "line": b.line.iter().map(|&p| xyz(p)).collect::<Vec<_>>(),
                "arrow": b.arrow,
            })).collect::<Vec<_>>(),
        });
        junctions_json = json!(net
            .junctions
            .iter()
            .map(|j| {
                let p = g.frame.en_to_geo(j.en);
                json!({
                    "lat": round(p.lat, 7),
                    "lon": round(p.lon, 7),
                    "kind": j.kind,
                    "arms": j.arms,
                    "minAngleDeg": round(j.min_angle_deg, 1),
                    "status": j.status.as_str(),
                    "reason": j.reason,
                    "edges": j.arm_edges,
                })
            })
            .collect::<Vec<_>>());
    }

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
    if junctions > 0 && opts.network.is_none() {
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
                cl.min_width_m,
                cl.max_width_m
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
    let every = ((opts.control_every_m / cl.spacing_m).round() as usize).max(1);
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
        "sampleSpacing": cl.spacing_m,
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
        "junctions": junctions_json,
        "network": network_json,
        "source": {
            "generator": format!("track-compiler {}", crate::COMPILER_VERSION),
            "graph": format!("{}@{}", g.region, g.hash),
            "osmBaseTimestamp": g.source.osm_base_timestamp,
            "dem": { "product": "Copernicus GLO-30", "hash": dem.identity() },
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
    let mut json = json;
    if let Some(n) = &opts.network {
        // Receta de la red, para reconstruir la pista igual.
        json["source"]["network"] = json!({
            "sidewalkM": n.sidewalk_m,
            "curbRadiusM": n.curb_radius_m,
            "stubM": n.stub_m,
            "terrain": {
                "stepM": opts.terrain.step_m,
                "radiusM": opts.terrain.radius_m,
                "percentile": opts.terrain.percentile,
                "sigmaM": opts.terrain.sigma_m,
            },
        });
    }
    if let Some(obj) = json.as_object_mut() {
        obj.retain(|_, v| !v.is_null()); // pistas sin red: el JSON queda como en S2
    }
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
