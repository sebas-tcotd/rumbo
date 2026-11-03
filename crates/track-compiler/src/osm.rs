//! Lectura de extractos de OpenStreetMap en formato Overpass JSON.
//!
//! Se eligió Overpass JSON para S1 porque es simple de pedir para una caja y de archivar.
//! Un lector de `.osm.pbf` puede sumarse después sin tocar el resto del pipeline.

use serde::Deserialize;
use std::collections::BTreeMap;

pub type Tags = BTreeMap<String, String>;

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Element {
    Node {
        id: i64,
        lat: f64,
        lon: f64,
        #[serde(default)]
        tags: Tags,
    },
    Way {
        id: i64,
        nodes: Vec<i64>,
        #[serde(default)]
        tags: Tags,
    },
    Relation {
        id: i64,
        #[serde(default)]
        tags: Tags,
    },
}

#[derive(Debug, Deserialize, Default)]
pub struct Osm3s {
    #[serde(default)]
    pub timestamp_osm_base: Option<String>,
    #[serde(default)]
    pub copyright: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct OverpassDoc {
    #[serde(default)]
    pub osm3s: Osm3s,
    pub elements: Vec<Element>,
}

#[derive(Debug, Clone)]
pub struct Node {
    pub lat: f64,
    pub lon: f64,
}

#[derive(Debug, Clone)]
pub struct Way {
    pub id: i64,
    pub nodes: Vec<i64>,
    pub tags: Tags,
}

/// Extracto ya ordenado: nodos por id y vías por id (orden fijo para el hash).
#[derive(Debug, Default)]
pub struct Extract {
    pub nodes: BTreeMap<i64, Node>,
    pub ways: Vec<Way>,
    pub relations: usize,
    pub osm_base_timestamp: Option<String>,
}

impl Extract {
    pub fn from_overpass_json(s: &str) -> Result<Self, String> {
        let doc: OverpassDoc = serde_json::from_str(s).map_err(|e| format!("Overpass JSON inválido: {e}"))?;
        let mut ex = Extract { osm_base_timestamp: doc.osm3s.timestamp_osm_base, ..Default::default() };
        for el in doc.elements {
            match el {
                Element::Node { id, lat, lon, .. } => {
                    ex.nodes.insert(id, Node { lat, lon });
                }
                Element::Way { id, nodes, tags } => ex.ways.push(Way { id, nodes, tags }),
                Element::Relation { .. } => ex.relations += 1,
            }
        }
        ex.ways.sort_by_key(|w| w.id);
        ex.ways.dedup_by_key(|w| w.id);
        Ok(ex)
    }
}
