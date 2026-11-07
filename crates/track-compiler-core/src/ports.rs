use crate::{osm::Extract, GeoPoint};

/// Supplies an ordered extract for a logical source reference.
pub trait OsmSource {
    /// # Errors
    /// Returns an error when acquisition or decoding fails.
    fn load(&self, reference: &str) -> Result<Extract, String>;
}

/// Synchronously samples a loaded elevation model.
pub trait ElevationSource {
    fn height(&self, point: GeoPoint) -> Option<f64>;
    fn identity(&self) -> &str;
}

/// Publishes the current compiled track representation.
pub trait ZoneWriter {
    /// # Errors
    /// Returns an error when publication fails.
    fn publish(&self, track: &serde_json::Value) -> Result<(), String>;
}
