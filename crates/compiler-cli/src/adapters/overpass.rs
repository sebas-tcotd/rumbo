use track_compiler_core::{osm::Extract, ports::OsmSource};
pub struct OverpassFile;
impl OsmSource for OverpassFile {
    fn load(&self, reference: &str) -> Result<Extract, String> {
        Extract::from_overpass_json(&super::fs::read(reference)?)
    }
}
