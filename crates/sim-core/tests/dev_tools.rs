//! AR28 dev tools: teleport and re-simulation up to a tick. Neither changes the normal tick.
use sim_core::replay::{resimulate_until, MAX_SEEK_TICKS};
use sim_core::{ProfileFile, ReplayManifest, Simulation, TrackDef};
use std::path::PathBuf;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn track(id: &str) -> TrackDef {
    TrackDef::from_json(&std::fs::read_to_string(root().join(format!("assets/tracks/{id}.json"))).unwrap()).unwrap()
}

fn golden(name: &str) -> ReplayManifest {
    ReplayManifest::from_json(&std::fs::read_to_string(root().join(format!("crates/sim-core/tests/golden/{name}.json"))).unwrap())
        .unwrap()
}

#[test]
fn resimulating_a_golden_reaches_its_checkpoint_hashes() {
    let manifest = golden("test-loop.scripted");
    let def = track("test-loop");
    for check in manifest.checks.iter().take(3) {
        let sim = resimulate_until(&manifest, &def, check.tick).unwrap();
        assert_eq!(sim.tick(), check.tick);
        assert_eq!(sim.state_hash_hex(), check.hash, "tick {}", check.tick);
    }
    let end = resimulate_until(&manifest, &def, manifest.ticks).unwrap();
    assert_eq!(end.state_hash_hex(), manifest.final_state_hash);
    assert_eq!(resimulate_until(&manifest, &def, 0).unwrap().tick(), 0);
}

#[test]
fn resimulation_rejects_out_of_range_ticks_and_foreign_replays() {
    let manifest = golden("test-loop.autopilot");
    let def = track("test-loop");
    assert!(resimulate_until(&manifest, &def, manifest.ticks + 1).is_err());
    assert!(MAX_SEEK_TICKS >= manifest.ticks);
    let mut long = manifest.clone();
    long.ticks = MAX_SEEK_TICKS + 1;
    assert!(resimulate_until(&long, &def, MAX_SEEK_TICKS + 1).is_err());
    assert!(resimulate_until(&manifest, &track("costa-verde-sprint"), 10).is_err());
    let mut other = manifest.clone();
    other.format = "otro/1".into();
    assert!(resimulate_until(&other, &def, 10).is_err());
    let mut physics = manifest.clone();
    physics.physics_version = "0.0.0".into();
    assert!(resimulate_until(&physics, &def, 10).is_err());
}

#[test]
fn resimulation_is_bounded_by_the_recorded_inputs_not_the_declared_ticks() {
    let def = track("test-loop");
    let mut truncated = golden("test-loop.scripted");
    let recorded: u64 = truncated.inputs.iter().map(|run| u64::from(run[0])).sum();
    // Declares more ticks than it records: a seek past the inputs is refused, never short.
    truncated.ticks = recorded + 500;
    assert!(resimulate_until(&truncated, &def, recorded + 1).is_err());
    assert_eq!(resimulate_until(&truncated, &def, recorded).unwrap().tick(), recorded);
    // Declares fewer ticks than it records: a tick inside the inputs is still reachable.
    let mut short = golden("test-loop.scripted");
    short.ticks = recorded / 2;
    assert_eq!(resimulate_until(&short, &def, recorded - 1).unwrap().tick(), recorded - 1);
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
