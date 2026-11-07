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

/// Acelera en la recta hasta `kmh` y devuelve la sesión lista para la maniobra.
fn at_speed(profile: sim_core::ProfileFile, kmh: f32) -> sim_core::Simulation {
    let mut s = sim_core::Simulation::new(&track(), &profile, 7).unwrap();
    while state(&s)[SPEED] < kmh {
        s.step(InputFrame { throttle: 255, ..Default::default() });
    }
    s
}

/// Maniobra estándar: 20 ticks de `entry` para superar la rampa de dirección,
/// luego acelerador sostenido hasta completar 1 s.
/// Devuelve (entró en derrape, ángulo máximo).
fn maneuver(s: &mut sim_core::Simulation, entry: InputFrame) -> (bool, f32) {
    let mut max_slip = 0.0f32;
    let mut entered = false;
    for t in 0..HZ {
        let i = if t < 20 { InputFrame { steer: 127, ..entry } } else { InputFrame { steer: 127, throttle: 255, ..Default::default() } };
        s.step(i);
        let st = state(s);
        entered |= st[DRIFT] > 0.5;
        max_slip = max_slip.max(st[SLIP].abs());
        assert!(st[SPEED] > 60.0, "perdió demasiada velocidad");
    }
    (entered, max_slip)
}

#[test]
fn brake_and_steer_turns_without_drifting() {
    let mut s = at_speed(profile(), 110.0);
    let (entered, max_slip) = maneuver(&mut s, InputFrame { brake: 255, ..Default::default() });
    assert!(!entered, "frenar en curva no debería iniciar un derrape");
    assert!(max_slip < 12.0, "frenando en curva deslizó {max_slip:.1}°");
}

#[test]
fn handbrake_and_steer_starts_a_controlled_drift() {
    let mut s = at_speed(profile(), 110.0);
    let (entered, max_slip) = maneuver(&mut s, InputFrame { flags: sim_core::input::FLAG_HANDBRAKE, ..Default::default() });
    assert!(entered, "el freno de mano no inició el derrape");
    assert!((15.0..40.0).contains(&max_slip), "ángulo de derrape fuera de rango: {max_slip:.1}°");
}

#[test]
fn released_handbrake_does_not_trigger_delayed_drift() {
    let mut s = at_speed(profile(), 110.0);
    s.step(InputFrame { throttle: 255, flags: sim_core::input::FLAG_HANDBRAKE, ..Default::default() });
    for _ in 0..40 {
        s.step(InputFrame { throttle: 255, steer: 127, ..Default::default() });
        assert!(!s.vehicle().drift, "un toque anterior no debe iniciar un derrape tardío");
    }
}

#[test]
fn drift_assist_releases_after_handbrake_release() {
    let mut s = at_speed(profile(), 110.0);
    for _ in 0..20 {
        s.step(InputFrame { throttle: 255, steer: 127, flags: sim_core::input::FLAG_HANDBRAKE, ..Default::default() });
    }
    assert!(s.vehicle().drift);
    for _ in 0..HZ {
        s.step(InputFrame { throttle: 255, steer: -127, ..Default::default() });
    }
    assert!(!s.vehicle().drift, "el contravolante debe liberar la asistencia de derrape");
    assert!(state(&s).iter().all(|v| v.is_finite()));
}

#[test]
fn brake_tap_drift_is_an_option_of_the_profile() {
    let mut p = profile();
    p.params.drift_brake_tap = 1.0;
    let mut s = at_speed(p, 110.0);
    let (entered, _) = maneuver(&mut s, InputFrame { brake: 255, ..Default::default() });
    assert!(entered, "con la opción activa, el toque de freno debería iniciar el derrape");
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
