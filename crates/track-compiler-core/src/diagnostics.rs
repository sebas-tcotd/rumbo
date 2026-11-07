//! Diagnósticos del compilador.
//!
//! La severidad es independiente del estado de una pista (VALID / DEGRADED / UNSUPPORTED,
//! que llega con S2 y S3): un ancho estimado es informativo y no degrada nada.

use crate::geo::GeoPoint;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Info,
    Warning,
    Error,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub severity: Severity,
    /// Código estable, para filtrar o contar (p. ej. "width-estimated").
    pub code: String,
    pub message: String,
    /// Vía de OSM afectada, si aplica.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub way: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub at: Option<GeoPoint>,
}

impl Diagnostic {
    pub fn new(severity: Severity, code: &str, message: impl Into<String>) -> Self {
        Self { severity, code: code.into(), message: message.into(), way: None, at: None }
    }

    pub fn way(mut self, id: i64) -> Self {
        self.way = Some(id);
        self
    }

    pub fn at(mut self, p: GeoPoint) -> Self {
        self.at = Some(p);
        self
    }
}
