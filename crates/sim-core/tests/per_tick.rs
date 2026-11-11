//! Per-tick state trace fingerprints for experimental physics 0.5.0.
//! Hash every little-endian u64 state hash, including all ticks between replay checks.
#![allow(clippy::disallowed_methods, reason = "test harness outside the core: it reads fixtures from disk (golden.rs also honours an env var to record them)")]
use sim_core::{InputFrame, ReplayManifest, Simulation, TrackDef};
use std::{fs, path::PathBuf};
#[test]
fn all_golden_ticks_match_handling_baseline() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    const EXPECTED: &[(&str, &str)] = &[
        ("costa-verde-sprint.adversarial.json", "b38f5a8a1a371080"),
        ("costa-verde-sprint.autopilot.json", "1e25a9c60c0306ae"),
        ("san-borja-las-artes.adversarial.json", "710464ab07f20e41"),
        ("san-borja-las-artes.autopilot.json", "2ee00bdca1af0f3c"),
        ("san-borja-las-artes.scripted.json", "d539b24dfc054a60"),
        ("test-loop.adversarial.json", "7cff1c5c613c2072"),
        ("test-loop.autopilot.json", "04fa2f51034dabdb"),
        ("test-loop.scripted.json", "a5d492d2d8eedb4e"),
    ];
    let mut files: Vec<_> = fs::read_dir(root.join("crates/sim-core/tests/golden"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    files.sort();
    assert_eq!(files.len(), EXPECTED.len());
    for path in files {
        let m = ReplayManifest::from_json(&fs::read_to_string(&path).unwrap()).unwrap();
        let t = TrackDef::from_json(
            &fs::read_to_string(root.join(format!("assets/tracks/{}.json", m.track_id))).unwrap(),
        )
        .unwrap();
        let mut sim = Simulation::new(&t, &m.profile, m.seed).unwrap();
        let mut trace = Vec::new();
        for run in &m.inputs {
            for _ in 0..run[0] {
                sim.step(InputFrame::unpack(run[1]));
                trace.extend_from_slice(&sim.state_hash().to_le_bytes());
            }
        }
        let actual = sim_core::hash::Fnv64::new().bytes(&trace).hex();
        let name = path.file_name().unwrap().to_str().unwrap();
        let expected = EXPECTED.iter().find(|(n, _)| *n == name).unwrap().1;
        assert_eq!(actual, expected, "per-tick baseline: {name}");
        println!(
            "{} track={} profile={} final={} ticks={}",
            path.file_name().unwrap().to_string_lossy(),
            sim.track_hash_hex(),
            sim.profile_hash_hex(),
            sim.state_hash_hex(),
            sim.tick()
        );
    }
}
