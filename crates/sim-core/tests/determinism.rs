//! Determinismo y replays (S4 arranca junto con S0).
mod common;
use common::*;
use sim_core::input::FLAG_RESET;
use sim_core::{InputFrame, ReplayManifest, STATE_LEN};
use std::path::PathBuf;

fn golden_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden/autopilot-test-loop.json")
}

#[test]
fn same_inputs_same_hashes() {
    let mut a = sim();
    let mut b = sim();
    run_autopilot(&mut a, HZ * 45);
    run_autopilot(&mut b, HZ * 45);
    assert_eq!(a.state_hash(), b.state_hash());
    assert_eq!(a.replay().checks, b.replay().checks);
}

#[test]
fn scripted_session_replays_exactly() {
    let mut s = sim();
    for t in 0..HZ * 60 {
        s.step(scripted_input(t));
    }
    let manifest = s.replay();
    let json = serde_json::to_string(&manifest).unwrap();
    let back = ReplayManifest::from_json(&json).unwrap();
    let report = back.verify(&track()).unwrap();
    assert!(report.ok, "{}", report.message);
    assert_eq!(report.actual_final, manifest.final_state_hash);
}

#[test]
fn tampered_input_is_detected_at_its_check() {
    let mut s = sim();
    run_autopilot(&mut s, HZ * 30);
    let mut manifest = s.replay();
    // Cambia una sola entrada cerca del tick 1000.
    let mut acc = 0u64;
    for run in manifest.inputs.iter_mut() {
        acc += run[0] as u64;
        if acc > 1000 {
            let mut f = InputFrame::unpack(run[1]);
            f.steer = f.steer.saturating_add(40);
            run[1] = f.pack();
            break;
        }
    }
    let report = manifest.verify(&track()).unwrap();
    assert!(!report.ok);
    assert_eq!(report.first_mismatch_tick, Some(1200), "debería divergir en el primer control tras el cambio");
}

#[test]
fn tampered_profile_is_rejected() {
    let mut s = sim();
    run_autopilot(&mut s, HZ * 5);
    let mut manifest = s.replay();
    manifest.profile.params.rear_grip += 0.05;
    let report = manifest.verify(&track()).unwrap();
    assert!(!report.ok);
}

#[test]
fn adversarial_inputs_stay_finite_and_deterministic() {
    // Choques a fondo, volante al tope, frenadas bruscas y resets repetidos.
    let pattern = |t: u64| {
        let steer = if (t / 90) % 2 == 0 { 127 } else { -127 };
        let flags = if t % 1500 == 1499 { FLAG_RESET } else { 0 };
        let brake = if t % 400 < 10 { 255 } else { 0 };
        InputFrame { steer, throttle: 255, brake, flags }
    };
    let run = || {
        let mut s = sim();
        let mut st = [0.0f32; STATE_LEN];
        for t in 0..HZ * 40 {
            s.step(pattern(t));
            s.write_state(&mut st);
            assert!(st.iter().all(|v| v.is_finite()), "estado no finito en tick {t}");
        }
        assert!(!s.invalid(), "la simulación produjo NaN y tuvo que recolocar el auto");
        s.state_hash()
    };
    assert_eq!(run(), run());
}

#[test]
fn golden_replay_matches() {
    let path = golden_path();
    if std::env::var("UPDATE_GOLDEN").is_ok() || !path.exists() {
        let mut s = sim();
        run_autopilot(&mut s, HZ * 60);
        let json = serde_json::to_string(&s.replay()).unwrap();
        std::fs::write(&path, json).unwrap();
        if std::env::var("UPDATE_GOLDEN").is_err() {
            panic!("no había replay dorado; se generó en {}. Vuelve a correr las pruebas.", path.display());
        }
    }
    let manifest = ReplayManifest::from_json(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let report = manifest.verify(&track()).unwrap();
    assert!(report.ok, "{}: esperado {}, obtenido {}", report.message, report.expected_final, report.actual_final);
}
