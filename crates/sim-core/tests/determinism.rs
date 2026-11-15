//! Determinismo, replays `rumbo-replay/2` y la entrada canónica a 60 Hz (D7, D17).
mod common;
use common::*;
use sim_core::input::{FLAG_HANDBRAKE, FLAG_RESET, FLAG_RESTART};
use sim_core::replay::VehicleRef;
use sim_core::{InputFrame, ProfileFile, ReplayV2, STATE_LEN};

#[test]
fn same_inputs_same_hashes() {
    let mut a = sim();
    let mut b = sim();
    run_autopilot(&mut a, HZ * 45);
    run_autopilot(&mut b, HZ * 45);
    assert_eq!(a.state_hash(), b.state_hash());
    assert_eq!(a.replay().checks, b.replay().checks);
    assert_eq!(a.replay().encode().unwrap(), b.replay().encode().unwrap());
}

#[test]
fn scripted_session_replays_exactly_through_bytes() {
    let mut s = sim();
    for t in 0..HZ * 60 {
        s.step(scripted_input(t));
    }
    let replay = s.replay();
    assert_eq!(replay.checks.len(), 6, "a control hash every 10 s");
    assert_eq!(ReplayV2::frames_for(replay.ticks), HZ * 30, "one frame per pair of ticks");
    let back = ReplayV2::decode(&replay.encode().unwrap()).unwrap();
    assert_eq!(back, replay);
    let report = sim_core::verify_replay(&back, &track(), &profile()).unwrap();
    assert!(report.ok, "{}", report.message);
    assert!(report.checks_match);
    assert!(!report.profile_verified, "LEGACY_VEHICLE cannot verify the profile");
    assert_eq!(report.actual_final, format!("{:016x}", replay.final_hash));
}

#[test]
fn a_replay_may_end_on_the_first_tick_of_a_pair() {
    let mut s = sim();
    run_autopilot(&mut s, 301);
    let replay = s.replay();
    assert_eq!((replay.ticks, ReplayV2::frames_for(replay.ticks)), (301, 151));
    let report = sim_core::verify_replay(&ReplayV2::decode(&replay.encode().unwrap()).unwrap(), &track(), &profile());
    assert!(report.unwrap().ok);
}

#[test]
fn tampered_input_is_detected_at_its_check() {
    let mut s = sim();
    run_autopilot(&mut s, HZ * 30);
    let mut replay = s.replay();
    // Changes one run around tick 1000 (frame 500): the first control after it is tick 1200.
    let mut frames = 0;
    let run = replay.inputs.iter_mut().find(|r| {
        frames += r.frames;
        frames > 500
    });
    let run = run.unwrap();
    run.frame.steer = run.frame.steer.saturating_add(40);
    let report = sim_core::verify_replay(&replay, &track(), &profile()).unwrap();
    assert!(!report.ok);
    assert_eq!(report.first_mismatch_tick, Some(1200), "debería divergir en el primer control tras el cambio");
}

#[test]
fn a_different_profile_diverges_and_foreign_headers_are_rejected() {
    let mut s = sim();
    run_autopilot(&mut s, HZ * 12);
    let replay = s.replay();
    let mut other = profile();
    other.params.rear_grip += 0.05;
    assert!(!sim_core::verify_replay(&replay, &track(), &other).unwrap().ok);
    let rejected = |r: &ReplayV2| sim_core::verify_replay(r, &track(), &profile()).unwrap().ok;
    let mut physics = replay.clone();
    physics.header.physics_version = "0.0.0".into();
    let mut track_hash = replay.clone();
    track_hash.header.track_hash.0[0] ^= 1;
    let mut race = replay.clone();
    race.header.race_hash.0[0] ^= 1;
    let mut vehicle = replay.clone();
    vehicle.header.vehicle_ref = VehicleRef { id: "meridian-garua".into(), ..VehicleRef::legacy() };
    for bad in [physics, track_hash, race, vehicle] {
        assert!(!rejected(&bad));
    }
}

#[test]
fn second_tick_of_a_pair_ignores_its_argument() {
    let garbage = InputFrame { steer: -127, throttle: 0, brake: 255, flags: FLAG_RESTART };
    let mut a = sim();
    let mut b = sim();
    for t in 0..HZ * 20 {
        let input = scripted_input(t - t % 2);
        a.step(input);
        b.step(if t % 2 == 0 { input } else { garbage });
    }
    assert_eq!(a.state_hash(), b.state_hash());
    assert_eq!(a.replay(), b.replay());
}

/// The pairing rule is exactly "frame on the first tick, frame without events on the second".
#[test]
fn pairs_apply_axes_and_handbrake_twice_and_events_once() {
    let frame = |k: u64| {
        let mut flags = if k % 40 < 10 { FLAG_HANDBRAKE } else { 0 };
        if k % 300 == 299 {
            flags |= FLAG_RESET;
        }
        if k % 700 == 699 {
            flags |= FLAG_RESTART;
        }
        InputFrame { steer: if (k / 45).is_multiple_of(2) { 90 } else { -60 }, throttle: 220, brake: 0, flags }
    };
    let mut paired = sim();
    let mut legacy = sim();
    for k in 0..HZ * 15 {
        let f = frame(k);
        paired.step(f);
        paired.step(f);
        legacy.step_legacy_v1(f);
        legacy.step_legacy_v1(f.held());
    }
    assert_eq!(paired.state_hash(), legacy.state_hash());
    let mut doubled = sim();
    for k in 0..HZ * 15 {
        doubled.step_legacy_v1(frame(k));
        doubled.step_legacy_v1(frame(k));
    }
    assert_ne!(paired.state_hash(), doubled.state_hash(), "events must not repeat on the second tick");
}

#[test]
fn accepts_frame_on_even_ticks_only() {
    let mut s = sim();
    for t in 0..6 {
        assert_eq!(s.accepts_frame(), t % 2 == 0);
        s.step(InputFrame::default());
    }
}

#[test]
fn adversarial_inputs_stay_finite_and_deterministic() {
    // Choques a fondo, volante al tope, frenadas bruscas y resets repetidos.
    let pattern = |t: u64| {
        let steer = if (t / 90).is_multiple_of(2) { 127 } else { -127 };
        let flags = if t % 1500 == 1498 { FLAG_RESET } else { 0 };
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
fn profile_file_is_kept_for_verification() {
    let s = sim();
    assert_eq!(s.profile_file(), &ProfileFile::default());
}
