//! S3: circuito urbano con cruces (San Borja, Las Artes Norte).
//!
//! Usa `assets/tracks/san-borja-las-artes.json`, generada con `track-compiler track --network`.
//! El piloto automático da vueltas completas pasando por cruces en T y de cuatro brazos,
//! girando y siguiendo derecho: sin saltos, sin ruedas en el aire y sin tocar muros.
#![allow(clippy::unwrap_used, clippy::expect_used, reason = "test code: a panic is the failure report")]
use sim_core::autopilot::Autopilot;
use sim_core::race::LapEvent;
use sim_core::{ProfileFile, Simulation, TrackDef, STATE_LEN};

const HZ: u64 = 120;

fn track() -> TrackDef {
    TrackDef::from_json(include_str!("../../../assets/tracks/san-borja-las-artes.json")).unwrap()
}

#[test]
fn network_compiles_into_surface_and_walls() {
    let def = track();
    let net = def.network.as_ref().expect("la pista trae su red");
    let t = sim_core::compile_track(&def).unwrap();
    assert!(t.closed && t.network);
    assert_eq!(t.drivable.indices.len(), net.triangles.len() * 3);
    assert!(t.wall_boxes.len() > 100, "{} muros", t.wall_boxes.len());
    // El suelo queda por debajo de toda la red.
    assert!(net.vertices.iter().all(|v| v[1] > t.ground_y));
}

#[test]
fn autopilot_laps_through_the_junctions() {
    let mut sim = Simulation::new(&track(), &ProfileFile::default(), 7).unwrap();
    let ap = Autopilot::default();
    let mut st = [0.0f32; STATE_LEN];
    let (mut prev_vy, mut max_jolt, mut airborne, mut wall, mut max_kmh) = (0.0f32, 0.0f32, 0u64, 0u64, 0.0f32);
    let mut laps = Vec::new();
    let mut invalid = 0;
    for _ in 0..HZ * 300 {
        let i = ap.input(&sim);
        sim.step(i);
        sim.write_state(&mut st);
        max_jolt = max_jolt.max((st[9] - prev_vy).abs());
        prev_vy = st[9];
        max_kmh = max_kmh.max(st[11]);
        if st[16..20].iter().sum::<f32>() < 4.0 {
            airborne += 1;
        }
        if sim.vehicle().touching_wall {
            wall += 1;
        }
        match sim.last_event() {
            LapEvent::Completed { ticks, .. } => laps.push(ticks as f32 / HZ as f32),
            LapEvent::Invalidated => invalid += 1,
            _ => {}
        }
        if laps.len() == 3 {
            break;
        }
    }
    eprintln!(
        "vueltas {laps:?} s · máx {max_kmh:.0} km/h · golpe vertical máx {max_jolt:.2} m/s · {airborne} ticks sin las 4 ruedas · {wall} ticks contra muro"
    );
    assert_eq!(laps.len(), 3, "no completó 3 vueltas");
    assert_eq!(invalid, 0);
    assert!(!sim.invalid());
    assert_eq!(airborne, 0, "ruedas en el aire: hay un escalón en la superficie");
    assert_eq!(wall, 0, "el piloto automático tocó un muro");
    assert!(max_jolt < 0.5, "golpe vertical de {max_jolt} m/s");
}

/// A fondo y derecho: en la T de Monet el circuito dobla y la calle que sigue está cerrada por
/// una barrera. El auto se detiene en ella y **R** lo devuelve al último punto donde estuvo
/// sobre la pista, nunca más adelante.
#[test]
fn barrier_holds_and_reset_never_moves_ahead() {
    use sim_core::input::{InputFrame, FLAG_RESET};
    let def = track();
    assert_eq!(def.network.as_ref().unwrap().barriers.len(), 9, "una barrera por calle lateral");
    let mut sim = Simulation::new(&def, &ProfileFile::default(), 3).unwrap();
    let gas = InputFrame { steer: 0, throttle: 255, brake: 0, flags: 0 };
    let mut st = [0.0f32; STATE_LEN];
    let (mut max_s, mut hit) = (0.0f32, false);
    for _ in 0..HZ * 10 {
        sim.step(gas);
        sim.write_state(&mut st);
        if st[33] < 300.0 {
            max_s = max_s.max(st[33]);
        }
        hit |= sim.vehicle().touching_wall;
    }
    assert!(hit, "no llegó a la barrera");
    let before = sim.view().pos;
    sim.step(InputFrame { flags: FLAG_RESET, ..InputFrame::default() });
    sim.write_state(&mut st);
    let after = sim.view().pos;
    let s_after = st[33];
    eprintln!("avance máximo {max_s:.0} m · tras R en s = {s_after:.0} m · se movió {:.1} m", (after - before).length());
    assert!(s_after <= max_s + 1.0, "R adelantó el auto: {s_after} > {max_s}");
    assert!(max_s < 200.0, "pasó la barrera: avanzó hasta s = {max_s}");
}
