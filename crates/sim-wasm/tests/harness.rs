//! Story 0.9: the harness trace (what the WASM test binary exports and the native reference records) has
//! exactly the per-tick semantics pinned by `crates/sim-core/tests/per_tick.rs`: hashing each tick's
//! `u64` little-endian with FNV gives the same fingerprints. Run with `--features harness`.
#![allow(clippy::unwrap_used, clippy::expect_used, reason = "test code: a panic is the failure report")]
use sim_core::hash::Fnv64;
use sim_wasm::harness::{trace, HEX_PER_TICK};
use std::path::PathBuf;

/// Same table as per_tick.rs (physics 0.5.0).
const EXPECTED: &[(&str, &str)] = &[
    ("costa-verde-sprint.adversarial", "b38f5a8a1a371080"),
    ("costa-verde-sprint.autopilot", "1e25a9c60c0306ae"),
    ("san-borja-las-artes.adversarial", "710464ab07f20e41"),
    ("san-borja-las-artes.autopilot", "2ee00bdca1af0f3c"),
    ("san-borja-las-artes.scripted", "d539b24dfc054a60"),
    ("test-loop.adversarial", "7cff1c5c613c2072"),
    ("test-loop.autopilot", "04fa2f51034dabdb"),
    ("test-loop.scripted", "a5d492d2d8eedb4e"),
];

#[test]
fn harness_trace_matches_per_tick_fingerprints() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    for (name, expected) in EXPECTED {
        let replay = std::fs::read_to_string(root.join(format!("crates/sim-core/tests/golden/{name}.json"))).unwrap();
        let manifest: serde_json::Value = serde_json::from_str(&replay).unwrap();
        let track_id = manifest["trackId"].as_str().unwrap();
        let track = std::fs::read_to_string(root.join(format!("assets/tracks/{track_id}.json"))).unwrap();
        let t = trace(&track, &replay).unwrap();
        assert_eq!(t.ticks, manifest["ticks"].as_u64().unwrap(), "{name}: ticks");
        assert_eq!(t.hex.len() as u64, t.ticks * HEX_PER_TICK as u64, "{name}: one hash per tick");
        let mut bytes = Vec::with_capacity(t.hex.len() / 2);
        for chunk in t.hex.as_bytes().chunks(HEX_PER_TICK) {
            let h = u64::from_str_radix(std::str::from_utf8(chunk).unwrap(), 16).unwrap();
            bytes.extend_from_slice(&h.to_le_bytes());
        }
        assert_eq!(Fnv64::new().bytes(&bytes).hex(), *expected, "{name}: per-tick fingerprint");
        assert_eq!(&t.hex[t.hex.len() - HEX_PER_TICK..], manifest["finalStateHash"].as_str().unwrap(), "{name}: final");
    }
}

#[test]
fn harness_rejects_other_physics_versions() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let replay = std::fs::read_to_string(root.join("crates/sim-core/tests/golden/test-loop.scripted.json")).unwrap();
    let track = std::fs::read_to_string(root.join("assets/tracks/test-loop.json")).unwrap();
    let mut manifest: serde_json::Value = serde_json::from_str(&replay).unwrap();
    manifest["physicsVersion"] = "0.4.0".into();
    let other = manifest.to_string();
    assert_eq!(trace(&track, &other).unwrap_err().code(), "INVALID_REPLAY");
}
