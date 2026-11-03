//! Pruebas de manejo: sanidad física y la "prueba de manejo automática".
mod common;
use common::*;
use sim_core::race::LapEvent;
use sim_core::{InputFrame, STATE_LEN};

fn state(s: &sim_core::Simulation) -> [f32; STATE_LEN] {
    let mut st = [0.0f32; STATE_LEN];
    s.write_state(&mut st);
    st
}

const SPEED: usize = 11;
const SLIP: usize = 12;
const DRIFT: usize = 15;
const CONTACTS: std::ops::Range<usize> = 16..20;
const VY: usize = 9;

#[test]
fn settles_at_rest_on_four_wheels() {
    let mut s = sim();
    for _ in 0..HZ * 2 {
        s.step(InputFrame::default());
    }
    let st = state(&s);
    assert!(st[SPEED].abs() < 0.5, "debería quedar quieto: {} km/h", st[SPEED]);
    assert_eq!(st[CONTACTS].iter().sum::<f32>(), 4.0);
}

#[test]
fn accelerates_0_100_in_a_plausible_time() {
    let mut s = sim();
    for _ in 0..HZ {
        s.step(InputFrame::default());
    }
    let full = InputFrame { throttle: 255, ..Default::default() };
    let mut t100 = None;
    for t in 0..HZ * 12 {
        s.step(full);
        if t100.is_none() && state(&s)[SPEED] >= 100.0 {
            t100 = Some(t as f32 / HZ as f32);
        }
    }
    let t100 = t100.expect("nunca llegó a 100 km/h");
    assert!((3.0..8.0).contains(&t100), "0-100 en {t100:.2} s");
}

/// La "prueba de manejo automática" del documento: un auto sin conductor recorre la pista
/// y cualquier golpe vertical o pérdida de contacto anómala falla la prueba.
#[test]
fn autopilot_laps_are_smooth_and_valid() {
    let mut s = sim();
    let ap = sim_core::autopilot::Autopilot::default();
    let mut prev_vy = 0.0f32;
    let mut max_jolt = 0.0f32;
    let mut airborne_ticks = 0u64;
    let mut wall_ticks = 0u64;
    let total = HZ * 200;
    for _ in 0..total {
        let i = ap.input(&s);
        s.step(i);
        let st = state(&s);
        max_jolt = max_jolt.max((st[VY] - prev_vy).abs());
        prev_vy = st[VY];
        if st[CONTACTS].iter().sum::<f32>() < 4.0 {
            airborne_ticks += 1;
        }
        if s.vehicle().touching_wall {
            wall_ticks += 1;
        }
        assert!(!matches!(s.last_event(), LapEvent::Invalidated), "vuelta anulada");
    }
    let laps = &s.timer().completed;
    assert!(laps.len() >= 3, "vueltas completas: {}", laps.len());
    let (a, b) = (laps[1] as f32, laps[2] as f32);
    assert!((a - b).abs() / a < 0.01, "vueltas inconsistentes: {a} vs {b}");
    assert!(max_jolt < 0.6, "golpe vertical de {max_jolt:.2} m/s en un tick");
    assert!((airborne_ticks as f32) < total as f32 * 0.01, "ruedas sin contacto en {airborne_ticks} ticks");
    assert_eq!(wall_ticks, 0, "el piloto automático tocó el muro");
}

#[test]
fn brake_tap_starts_a_controlled_drift() {
    let mut s = sim();
    while state(&s)[SPEED] < 110.0 {
        s.step(InputFrame { throttle: 255, ..Default::default() });
    }
    let mut max_slip = 0.0f32;
    let mut entered = false;
    for t in 0..HZ {
        let brake = if t < 10 { 255 } else { 0 };
        s.step(InputFrame { steer: 127, throttle: if t >= 10 { 255 } else { 0 }, brake, flags: 0 });
        let st = state(&s);
        entered |= st[DRIFT] > 0.5;
        max_slip = max_slip.max(st[SLIP].abs());
        assert!(st[SPEED] > 60.0, "perdió demasiada velocidad derrapando");
    }
    assert!(entered, "el toque de freno no inició el derrape");
    assert!((15.0..40.0).contains(&max_slip), "ángulo de derrape fuera de rango: {max_slip:.1}°");
}

#[test]
fn wall_scrape_does_not_launch_the_car() {
    let mut s = sim();
    for _ in 0..HZ * 5 {
        s.step(InputFrame { throttle: 255, ..Default::default() });
    }
    let mut touched = false;
    let mut max_y_gain = 0.0f32;
    let y0 = state(&s)[2];
    for t in 0..HZ * 3 {
        s.step(InputFrame { throttle: 255, steer: if t < 150 { 127 } else { 0 }, ..Default::default() });
        touched |= s.vehicle().touching_wall;
        max_y_gain = max_y_gain.max(state(&s)[2] - y0);
    }
    assert!(touched, "el escenario no llegó a tocar el muro");
    assert!(max_y_gain < 0.25, "el auto subió {max_y_gain:.2} m al tocar el muro");
}
