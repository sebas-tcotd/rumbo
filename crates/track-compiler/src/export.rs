//! Exportación a GeoJSON (RFC 7946) para revisar el grafo y las rutas sobre un mapa,
//! por ejemplo arrastrando el archivo a geojson.io.
//!
//! GeoJSON exige el orden [longitud, latitud]: es el único lugar del proyecto que lo usa.

use crate::geo::GeoPoint;
use crate::graph::{Oneway, RoadGraph};
use crate::routing::ResolvedRoute;
use serde_json::{json, Value};

fn coords(pts: &[GeoPoint]) -> Vec<[f64; 2]> {
    pts.iter().map(|p| [p.lon, p.lat]).collect()
}

/// Colores de la paleta del laboratorio (propiedades simplestyle de geojson.io).
fn style_for(e: &crate::graph::Edge) -> (&'static str, f64) {
    if e.bridge {
        ("#2f6f9f", 4.0)
    } else if e.tunnel {
        ("#7a7a7a", 4.0)
    } else {
        match e.class.as_str() {
            "motorway" | "trunk" | "primary" => ("#3b3e43", 3.5),
            "secondary" | "tertiary" => ("#5f676d", 2.5),
            _ => ("#9aa0a4", 1.5),
        }
    }
}

pub fn graph_geojson(g: &RoadGraph) -> Value {
    let features: Vec<Value> = g
        .edges
        .iter()
        .map(|e| {
            let (stroke, width) = style_for(e);
            json!({
                "type": "Feature",
                "geometry": { "type": "LineString", "coordinates": coords(&e.geo) },
                "properties": {
                    "id": e.id,
                    "way": e.way,
                    "class": e.class.as_str(),
                    "name": e.name,
                    "oneway": match e.oneway { Oneway::Both => "no", Oneway::Forward => "sí", Oneway::Backward => "sí (invertido)" },
                    "lanes": e.lanes,
                    "widthM": (e.width_m * 100.0).round() / 100.0,
                    "widthEstimated": e.width_estimated,
                    "bridge": e.bridge,
                    "tunnel": e.tunnel,
                    "layer": e.layer,
                    "lengthM": (e.length_m * 10.0).round() / 10.0,
                    "component": g.component[e.from],
                    "stroke": stroke,
                    "stroke-width": width,
                }
            })
        })
        .collect();
    json!({ "type": "FeatureCollection", "features": features })
}

pub fn route_geojson(r: &ResolvedRoute) -> Value {
    let mut features = vec![json!({
        "type": "Feature",
        "geometry": { "type": "LineString", "coordinates": coords(&r.geo) },
        "properties": {
            "name": "Ruta",
            "lengthM": (r.length_m * 10.0).round() / 10.0,
            "respectOneway": r.respect_oneway,
            "closed": r.closed,
            "stroke": "#e3a21a",
            "stroke-width": 6,
        }
    })];
    for (i, leg) in r.legs.iter().enumerate() {
        features.push(json!({
            "type": "Feature",
            "geometry": { "type": "Point", "coordinates": [leg.from.point.lon, leg.from.point.lat] },
            "properties": {
                "name": format!("Punto {} (ajustado)", i + 1),
                "offsetM": (leg.from.offset_m * 10.0).round() / 10.0,
                "marker-color": "#e3a21a",
            }
        }));
    }
    for (i, p) in r.waypoints.iter().enumerate() {
        features.push(json!({
            "type": "Feature",
            "geometry": { "type": "Point", "coordinates": [p.lon, p.lat] },
            "properties": { "name": format!("Punto {} (pedido)", i + 1), "marker-color": "#2a3035", "marker-size": "small" }
        }));
    }
    json!({ "type": "FeatureCollection", "features": features })
}
