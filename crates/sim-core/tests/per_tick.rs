//! Per-tick state trace fingerprints captured on main at 18649c3 before extraction.
//! Hash every little-endian u64 state hash, including all ticks between replay checks.
use sim_core::{InputFrame, ReplayManifest, Simulation, TrackDef};
use std::{fs, path::PathBuf};
#[test]
fn all_golden_ticks_match_pre_extraction_baseline() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    const EXPECTED: &[(&str, &str)] = &[
        ("costa-verde-sprint.adversarial.json", "bafe64bdd7b3cb15"),
        ("costa-verde-sprint.autopilot.json", "1e25a9c60c0306ae"),
        ("san-borja-las-artes.adversarial.json", "75b4e98aa3b47be3"),
        ("san-borja-las-artes.autopilot.json", "2ee00bdca1af0f3c"),
        ("san-borja-las-artes.scripted.json", "1eb3054a6edb2df5"),
        ("test-loop.adversarial.json", "16226ae5d51e35c6"),
        ("test-loop.autopilot.json", "04fa2f51034dabdb"),
        ("test-loop.scripted.json", "eacc58113dfd8513"),
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
