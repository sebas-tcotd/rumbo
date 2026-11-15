//! Story 0.9 (v2 since 0.10): the harness trace (what the WASM test binary exports and the native reference
//! records) has exactly the per-tick semantics pinned by `crates/sim-core/tests/per_tick.rs`: hashing each
//! tick's `u64` little-endian with FNV gives the same fingerprints. Run with `--features harness`.
#![allow(clippy::unwrap_used, clippy::expect_used, reason = "test code: a panic is the failure report")]
use sim_core::hash::Fnv64;
use sim_core::ReplayV2;
use sim_wasm::harness::{trace, HEX_PER_TICK};
use std::path::PathBuf;

/// Same table as per_tick.rs (physics 0.6.0).
const EXPECTED: &[(&str, &str)] = &[
    ("costa-verde-sprint.adversarial", "4cf46a7fe8d7afe0"),
    ("costa-verde-sprint.autopilot", "cd7bf6611b6c1413"),
    ("san-borja-las-artes.adversarial", "6ac3825f6d18bbde"),
    ("san-borja-las-artes.autopilot", "a9f1964e64e42955"),
    ("san-borja-las-artes.scripted", "5376e21f385871fe"),
    ("test-loop.adversarial", "d9fdbef754ac0983"),
    ("test-loop.autopilot", "0634882ad3d4a4af"),
    ("test-loop.scripted", "923794cdc01c459f"),
];

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn golden(name: &str) -> (String, Vec<u8>) {
    let replay = std::fs::read(root().join(format!("crates/sim-core/tests/golden/{name}.bin"))).unwrap();
    let track_id = name.split('.').next().unwrap();
    (std::fs::read_to_string(root().join(format!("assets/tracks/{track_id}.json"))).unwrap(), replay)
}

#[test]
fn harness_trace_matches_per_tick_fingerprints() {
    for (name, expected) in EXPECTED {
        let (track, replay) = golden(name);
        let decoded = ReplayV2::decode(&replay).unwrap();
        let t = trace(&track, &replay).unwrap();
        assert_eq!(t.ticks, decoded.ticks, "{name}: ticks");
        assert_eq!(t.hex.len() as u64, t.ticks * HEX_PER_TICK as u64, "{name}: one hash per tick");
        let mut bytes = Vec::with_capacity(t.hex.len() / 2);
        for chunk in t.hex.as_bytes().chunks(HEX_PER_TICK) {
            let h = u64::from_str_radix(std::str::from_utf8(chunk).unwrap(), 16).unwrap();
            bytes.extend_from_slice(&h.to_le_bytes());
        }
        assert_eq!(Fnv64::new().bytes(&bytes).hex(), *expected, "{name}: per-tick fingerprint");
        assert_eq!(&t.hex[t.hex.len() - HEX_PER_TICK..], format!("{:016x}", decoded.final_hash), "{name}: final");
    }
}

#[test]
fn harness_rejects_other_physics_versions_and_bad_bytes() {
    let (track, replay) = golden("test-loop.scripted");
    let mut decoded = ReplayV2::decode(&replay).unwrap();
    decoded.header.physics_version = "0.4.0".into();
    assert_eq!(trace(&track, &decoded.encode().unwrap()).unwrap_err().code(), "INVALID_REPLAY");
    assert_eq!(trace(&track, &replay[..replay.len() - 1]).unwrap_err().code(), "INVALID_REPLAY");
}

#[test]
fn summary_exposes_the_header_in_the_wire_shape() {
    let (_, replay) = golden("test-loop.autopilot");
    let s = sim_wasm::summary(&ReplayV2::decode(&replay).unwrap());
    assert_eq!(s["header"]["format"], "rumbo-replay/2");
    assert_eq!(s["header"]["vehicleRef"]["id"], "legacy-vehicle");
    assert_eq!(s["header"]["seed"], "7");
    assert_eq!(s["header"]["players"], 1);
    assert_eq!(s["ticks"], 7200);
    assert_eq!(s["frames"], 3600);
    assert_eq!(s["checks"].as_array().unwrap().len(), 6);
}
