use sim_core::{compile_track, TrackDef};
use std::{fs, process::Command};

#[test]
fn mini_grid_preserves_compiled_identity() {
    let dir = std::env::temp_dir().join(format!("rumbo-cli-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let invoke = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_rumbo-compiler"))
            .current_dir(&dir)
            .args(args)
            .output()
            .unwrap()
    };
    let run = |args: &[&str]| {
        let output = invoke(args);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    };
    run(&[
        "ingest",
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/mini-grid.overpass.json"
        ),
        "--region",
        "fixture",
        "--origin",
        "-12.12,-77.03",
        "--out",
        "graph.json",
    ]);
    run(&[
        "track",
        "graph.json",
        "--dem",
        concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/mini-grid.tif"),
        "--from",
        "-12.12,-77.03",
        "--to",
        "-12.118191354675348,-77.02816244027932",
        "--id",
        "fixture",
        "--out",
        "track.json",
        "--geojson",
        "route.geojson",
        "--profile",
        "elevation.csv",
    ]);
    let json = fs::read_to_string(dir.join("track.json")).unwrap();
    let track = TrackDef::from_json(&json).unwrap();
    assert_eq!(compile_track(&track).unwrap().hash, 0xd168e81a540b1014);
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["source"]["dem"]["hash"], "c212269b176d0d68");
    run(&["stats", "graph.json"]);
    run(&[
        "route",
        "graph.json",
        "--from",
        "-12.12,-77.03",
        "--to",
        "-12.118191354675348,-77.02816244027932",
        "--descriptor",
        "descriptor.json",
    ]);
    let read_json = |p: &str| {
        serde_json::from_slice::<serde_json::Value>(&fs::read(dir.join(p)).unwrap()).unwrap()
    };
    assert_eq!(read_json("descriptor.json"), value["source"]["route"]);
    assert_eq!(read_json("route.geojson")["type"], "FeatureCollection");
    assert!(
        fs::read_to_string(dir.join("elevation.csv"))
            .unwrap()
            .lines()
            .count()
            > 100
    );
    fs::write(dir.join("invalid"), b"broken").unwrap();
    for args in [
        vec!["stats", "missing"],
        vec!["stats", "invalid"],
        vec!["track", "graph.json", "--dem", "invalid"],
        vec![
            "ingest",
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/fixtures/mini-grid.overpass.json"
            ),
            "--region",
            "fixture",
            "--out",
            "invalid/graph",
        ],
    ] {
        assert!(!invoke(&args).status.success());
    }
    fs::remove_dir_all(dir).unwrap();
}

/// Replays the published recipes through the executable when the raw inputs are installed.
#[test]
fn published_recipes_preserve_compiled_identities() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    for (id, region, tile, hash) in [
        (
            "costa-verde-sprint",
            "lima-costa-verde",
            "078",
            0x438fcda7191f2309u64,
        ),
        (
            "san-borja-las-artes",
            "lima-san-borja",
            "077",
            0xbfdf9ded9a1c3155u64,
        ),
    ] {
        let osm = root.join(format!("data/raw/{region}-2026-11-04.overpass.json"));
        let dem = root.join(format!(
            "data/raw/Copernicus_DSM_COG_10_S13_00_W{tile}_00_DEM.tif"
        ));
        if !osm.exists() || !dem.exists() {
            continue;
        }
        let asset: serde_json::Value = serde_json::from_slice(
            &fs::read(root.join(format!("assets/tracks/{id}.json"))).unwrap(),
        )
        .unwrap();
        let dir = std::env::temp_dir().join(format!("rumbo-{id}-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let command = || {
            let mut cmd = Command::new(env!("CARGO_BIN_EXE_rumbo-compiler"));
            cmd.current_dir(&dir);
            cmd
        };
        assert!(command()
            .arg("ingest")
            .arg(osm)
            .args(["--region", region, "--out", "graph.json"])
            .status()
            .unwrap()
            .success());
        let source = &asset["source"];
        let waypoints = source["route"]["waypoints"].as_array().unwrap();
        let mut cmd = command();
        cmd.args(["track", "graph.json", "--dem"]).arg(dem).args([
            "--id",
            asset["id"].as_str().unwrap(),
            "--name",
            asset["name"].as_str().unwrap(),
            "--out",
            "track.json",
        ]);
        for (i, p) in waypoints.iter().enumerate() {
            let flag = if i == 0 {
                "--from"
            } else if i + 1 == waypoints.len() {
                "--to"
            } else {
                "--via"
            };
            cmd.arg(flag).arg(format!("{},{}", p["lat"], p["lon"]));
        }
        if source["route"]["closed"] == true {
            cmd.arg("--closed");
        }
        if source["route"]["resolvedPath"]["respectOneway"] == true {
            cmd.arg("--respect-oneway");
        }
        if source.get("network").is_some() {
            cmd.arg("--network");
        }
        for anchor in source["elevationAnchors"].as_array().unwrap() {
            cmd.arg("--anchor").arg(format!(
                "{},{},{}",
                anchor["lat"], anchor["lon"], anchor["heightM"]
            ));
        }
        assert!(cmd.status().unwrap().success());
        let result = fs::read_to_string(dir.join("track.json")).unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&result).unwrap(),
            asset
        );
        assert_eq!(
            compile_track(&TrackDef::from_json(&result).unwrap())
                .unwrap()
                .hash,
            hash
        );
        fs::remove_dir_all(dir).unwrap();
    }
}
