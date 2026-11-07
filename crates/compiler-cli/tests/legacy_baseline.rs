//! Expected identities captured from main 18649c3 before sharing FNV.
use track_compiler_core::{
    graph::{BuildOptions, RoadGraph},
    hash::Fnv64,
    osm::Extract,
    routing::{route, RouteOptions},
    GeoPoint,
};
#[test]
fn hashes_match_pre_extraction_baseline() {
    let ex = Extract::from_overpass_json(include_str!("fixtures/mini-grid.overpass.json")).unwrap();
    let g = RoadGraph::build(
        &ex,
        &BuildOptions {
            region: "fixture".into(),
            origin: Some(GeoPoint::new(-12.12, -77.03)),
        },
    )
    .unwrap();
    let r = route(
        &g,
        &[
            GeoPoint::new(-12.12, -77.03),
            GeoPoint::new(-12.12 + 200.0 / 110580.0, -77.03 + 200.0 / 108840.0),
        ],
        RouteOptions::default(),
    )
    .unwrap();
    assert_eq!(g.hash, "dac9a56416736e0b");
    let expected = serde_json::json!({"closed":false,"lengthM":400.1,"resolvedPath":{"edges":["+w20#0","+w20#1","+w12#0","+w12#1"],"graph":"fixture@dac9a56416736e0b","maxTurnDeg":135.0,"respectOneway":false,"turnCostM":20.0},"waypoints":[{"lat":-12.12,"lon":-77.03},{"lat":-12.118191354675348,"lon":-77.02816244027932}]});
    assert_eq!(r.descriptor_route(), expected);
    let mut h = Fnv64::default();
    assert_eq!(h.str_u64_len("legacy/1").hex(), "c46940799d17a32e");
    let p = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../data/raw/Copernicus_DSM_COG_10_S13_00_W078_00_DEM.tif"
    );
    if std::path::Path::new(p).exists() {
        assert_eq!(compiler_cli::adapters::geotiff::from_path(p).unwrap().hash, "f99b95c07027ea2a");
    }
}
