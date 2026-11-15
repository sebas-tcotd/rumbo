use track_compiler_core::{osm::Extract, ports::OsmSource};

use crate::RumboError;

pub struct OverpassFile;

impl OverpassFile {
    /// Reads and decodes an Overpass extract.
    /// # Errors
    /// Returns a file access error or an invalid extract error.
    pub fn load_file(&self, reference: &str) -> Result<Extract, RumboError> {
        let json = super::fs::read(reference).map_err(RumboError::Io)?;
        Extract::from_overpass_json(&json).map_err(RumboError::Compile)
    }
}

impl OsmSource for OverpassFile {
    fn load(&self, reference: &str) -> Result<Extract, String> {
        self.load_file(reference).map_err(|e| e.to_string())
    }
}
