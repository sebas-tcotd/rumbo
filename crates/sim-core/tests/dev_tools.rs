//! AR28 dev tools: teleport and re-simulation up to a tick. Neither changes the normal tick.
#![allow(
    clippy::disallowed_methods,
    reason = "test harness outside the core: it reads fixtures from disk (golden.rs also honours an env var to record them)"
)]
#![allow(clippy::unwrap_used, clippy::expect_used, reason = "test code: a panic is the failure report")]
use sim_core::replay::{resimulate_until, MAX_SEEK_TICKS};
use sim_core::{ProfileFile, ReplayV2, Simulation, TrackDef};
use std::path::PathBuf;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn track(id: &str) -> TrackDef {
    TrackDef::from_json(&std::fs::read_to_string(root().join(format!("assets/tracks/{id}.json"))).unwrap()).unwrap()
}

fn golden(name: &str) -> ReplayV2 {
    ReplayV2::decode(&std::fs::read(root().join(format!("crates/sim-core/tests/golden/{name}.bin"))).unwrap()).unwrap()
}

fn profile() -> ProfileFile {
    ProfileFile::default()
}

#[test]
fn resimulating_a_golden_reaches_its_checkpoint_hashes() {
    let replay = golden("test-loop.scripted");
    let def = track("test-loop");
    for check in replay.checks.iter().take(3) {
        let sim = resimulate_until(&replay, &def, &profile(), check.tick).unwrap();
        assert_eq!(sim.tick(), check.tick);
        assert_eq!(sim.state_hash(), check.hash, "tick {}", check.tick);
    }
    let end = resimulate_until(&replay, &def, &profile(), replay.ticks).unwrap();
    assert_eq!(end.state_hash(), replay.final_hash);
    assert_eq!(end.replay(), replay, "the re-simulated session records the same frames");
    assert_eq!(resimulate_until(&replay, &def, &profile(), 0).unwrap().tick(), 0);
}

#[test]
fn resimulation_stops_on_an_odd_tick_inside_a_pair() {
    let replay = golden("test-loop.scripted");
    let sim = resimulate_until(&replay, &track("test-loop"), &profile(), 1201).unwrap();
    assert_eq!(sim.tick(), 1201);
    assert!(!sim.accepts_frame());
}

#[test]
fn resimulation_rejects_out_of_range_ticks_and_foreign_replays() {
    let replay = golden("test-loop.autopilot");
    let def = track("test-loop");
    assert!(resimulate_until(&replay, &def, &profile(), replay.ticks + 1).is_err());
    assert!(MAX_SEEK_TICKS >= replay.ticks);
    let mut long = replay.clone();
    long.ticks = MAX_SEEK_TICKS + 1;
    assert!(resimulate_until(&long, &def, &profile(), MAX_SEEK_TICKS + 1).is_err());
    assert!(resimulate_until(&replay, &track("costa-verde-sprint"), &profile(), 10).is_err());
    let mut physics = replay.clone();
    physics.header.physics_version = "0.0.0".into();
    assert!(resimulate_until(&physics, &def, &profile(), 10).is_err());
}

#[test]
fn teleport_places_the_car_on_the_centerline_at_s_and_drops_the_attempt() {
    let def = track("test-loop");
    let mut sim = Simulation::new(&def, &ProfileFile::default(), 1).unwrap();
    let length = sim.track().length;
    for target in [0.25_f32, 0.6] {
        let s = length * target;
        sim.teleport_to_s(s);
        let view = sim.view();
        assert!((view.s - s).abs() < sim.track().spacing * 1.5, "s {} vs {}", view.s, s);
        assert!(view.vel.length() < 1e-3, "at rest");
    }
    // Wraps on a circuit instead of failing.
    sim.teleport_to_s(length + 10.0);
    assert!(sim.view().s < 20.0);
    sim.teleport_to_s(f32::NAN);
    sim.step(sim_core::InputFrame { steer: 0, throttle: 0, brake: 0, flags: 0 });
    assert!(!sim.invalid());
}
