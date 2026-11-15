//! Per-tick state trace fingerprints of the `rumbo-replay/2` goldens for physics 0.6.0 (story 0.10).
//! Hash every little-endian u64 state hash, including all ticks between replay checks.
#![allow(
    clippy::disallowed_methods,
    reason = "test harness outside the core: it reads fixtures from disk (golden.rs also honours an env var to record them)"
)]
use sim_core::{ProfileFile, ReplayV2, Simulation, TrackDef};
use std::{fs, path::PathBuf};
#[test]
fn all_golden_ticks_match_handling_baseline() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    const EXPECTED: &[(&str, &str)] = &[
        ("costa-verde-sprint.adversarial.bin", "4cf46a7fe8d7afe0"),
        ("costa-verde-sprint.autopilot.bin", "cd7bf6611b6c1413"),
        ("san-borja-las-artes.adversarial.bin", "6ac3825f6d18bbde"),
        ("san-borja-las-artes.autopilot.bin", "a9f1964e64e42955"),
        ("san-borja-las-artes.scripted.bin", "5376e21f385871fe"),
        ("test-loop.adversarial.bin", "d9fdbef754ac0983"),
        ("test-loop.autopilot.bin", "0634882ad3d4a4af"),
        ("test-loop.scripted.bin", "923794cdc01c459f"),
    ];
    let mut files: Vec<_> =
        fs::read_dir(root.join("crates/sim-core/tests/golden")).unwrap().map(|e| e.unwrap().path()).collect();
    files.sort();
    assert_eq!(files.len(), EXPECTED.len());
    let mut mismatches = Vec::new();
    for path in files {
        let name = path.file_name().unwrap().to_str().unwrap();
        let replay = ReplayV2::decode(&fs::read(&path).unwrap()).unwrap();
        let track_id = name.split('.').next().unwrap();
        let t = TrackDef::from_json(&fs::read_to_string(root.join(format!("assets/tracks/{track_id}.json"))).unwrap())
            .unwrap();
        let mut sim = Simulation::new(&t, &ProfileFile::default(), replay.header.seed).unwrap();
        let mut trace = Vec::new();
        for frame in replay.tick_frames() {
            sim.step(frame);
            trace.extend_from_slice(&sim.state_hash().to_le_bytes());
        }
        let actual = sim_core::hash::Fnv64::new().bytes(&trace).hex();
        println!("{name} fingerprint={actual} final={} ticks={}", sim.state_hash_hex(), sim.tick());
        let expected = EXPECTED.iter().find(|(n, _)| *n == name).unwrap().1;
        if actual != expected {
            mismatches.push(format!("{name}: {actual} (expected {expected})"));
        }
    }
    assert!(mismatches.is_empty(), "per-tick baseline: {mismatches:#?}");
}
