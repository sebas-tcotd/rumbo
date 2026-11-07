use std::cell::RefCell;
use track_compiler_core::{
    dem::Dem,
    osm::Extract,
    ports::{ElevationSource, OsmSource, ZoneWriter},
    routing::route,
    trackgen::{generate, TrackOptions},
    BuildOptions, GeoPoint, RoadGraph,
};

struct MemoryExtract;
impl OsmSource for MemoryExtract {
    fn load(&self, _: &str) -> Result<Extract, String> {
        Extract::from_overpass_json(include_str!(
            "../../compiler-cli/tests/fixtures/mini-grid.overpass.json"
        ))
    }
}

#[derive(Default)]
struct Capture(RefCell<Option<serde_json::Value>>);
impl ZoneWriter for Capture {
    fn publish(&self, track: &serde_json::Value) -> Result<(), String> {
        self.0.replace(Some(track.clone()));
        Ok(())
    }
}

#[test]
fn compilation_uses_memory_ports() {
    let dem = Dem::new(
        [2, 2],
        [-77.05, -12.10],
        [0.1, 0.1],
        vec![10., 20., 30., 40.],
        "memory".into(),
    )
    .unwrap();
    let source: &dyn ElevationSource = &dem;
    assert!((source.height(GeoPoint::new(-12.15, -77.0)).unwrap() - 25.).abs() < 1e-9);
    assert_eq!(source.height(GeoPoint::new(0., 0.)), None);
    let origin = GeoPoint::new(-12.12, -77.03);
    let graph = RoadGraph::build(
        &MemoryExtract.load("fixture").unwrap(),
        &BuildOptions {
            region: "fixture".into(),
            origin: Some(origin),
        },
    )
    .unwrap();
    let resolved = route(
        &graph,
        &[
            origin,
            GeoPoint::new(-12.118191354675348, -77.02816244027932),
        ],
        Default::default(),
    )
    .unwrap();
    let output = generate(
        &graph,
        &resolved,
        source,
        &TrackOptions::new("fixture", "fixture"),
    )
    .unwrap();
    let writer = Capture::default();
    writer.publish(&output.json).unwrap();
    assert_eq!(
        writer.0.borrow().as_ref().unwrap()["source"]["dem"]["hash"],
        "memory"
    );
}

#[test]
fn invalid_rasters_are_rejected() {
    for (dimensions, spacing) in [
        ([0, 0], [1., 1.]),
        ([2, 3], [1., 1.]),
        ([1, 1], [0., 1.]),
        ([1, 1], [1., f64::NAN]),
    ] {
        assert!(Dem::new(dimensions, [0., 0.], spacing, vec![1.], "test".into()).is_err());
    }
}
