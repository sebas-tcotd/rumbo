//! track-compiler: compilador de circuitos desde geografía real.
//!
//! S1 cubre la mitad "geográfica" del pipeline:
//! ingesta de OSM (Overpass JSON) → proyección WGS84 → ENU local → grafo vial dirigido
//! → ruteo A* entre puntos de paso → recorrido resuelto.
//! S2 agrega la mitad "conducible" para un tramo: eje suavizado, elevación filtrada desde
//! un modelo de elevación y exportación a una pista de sim-core (`trackgen`).
//! S3 agrega la red conducible de un circuito urbano: calles, cruces y calles laterales
//! (`network`), con la altura como un campo 2D suavizado (`terrain`).
//!
//! Convenciones:
//! - Coordenadas geográficas siempre como campos con nombre (`lat`, `lon`).
//!   Solo la exportación GeoJSON usa el orden `[lon, lat]` que exige el estándar.
//! - Espacio local en metros, `f64`, X este y Y norte. La altura llega en S2 con el
//!   modelo de elevación; aquí el grafo es plano.
//! - Todo lo que entra al hash se recorre en orden fijo (ids ordenados, `BTreeMap`).

pub mod centerline;
pub mod dem;
pub mod diagnostics;
pub mod elevation;
pub mod export;
pub mod geo;
pub mod graph;
pub mod hash;
pub mod network;
pub mod osm;
pub mod routing;
pub mod terrain;
pub mod trackgen;

pub use diagnostics::{Diagnostic, Severity};
pub use geo::{GeoPoint, LocalFrame};
pub use graph::{RoadGraph, BuildOptions};
pub use routing::{ResolvedRoute, RouteOptions};

pub const COMPILER_VERSION: &str = "0.2.0";
