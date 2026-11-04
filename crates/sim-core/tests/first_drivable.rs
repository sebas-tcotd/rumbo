//! First Drivable: la pista generada desde datos reales (S2) se carga y se recorre entera.
//!
//! Usa `assets/tracks/costa-verde-sprint.json`, generada con `track-compiler track`.
//! Falla ante saltos verticales, ruedas en el aire, valores inválidos o si el piloto
//! automático no llega a la meta: lo que en el documento es "un tramo real sin saltos ni huecos".
use sim_core::autopilot::Autopilot;
use sim_core::race::LapEvent;
use sim_core::{ProfileFile, Simulation, TrackDef, STATE_LEN};

const HZ: u64 = 120;

fn track() -> TrackDef {
    TrackDef::from_json(include_str!("../../../assets/tracks/costa-verde-sprint.json")).unwrap()
}

#[test]
fn costa_verde_compiles() {
    let t = track().compile().unwrap();
    assert!(!t.closed);
    assert!(t.length > 8000.0, "longitud {}", t.length);
    assert!(t.samples.iter().all(|s| s.pos.x.is_finite() && s.pos.y.is_finite() && s.pos.z.is_finite()));
}

#[test]
fn autopilot_drives_the_costa_verde_end_to_end() {
    let mut sim = Simulation::new(&track(), &ProfileFile::default(), 11).unwrap();
    let ap = Autopilot::default();
    let mut st = [0.0f32; STATE_LEN];
    let (mut prev_vy, mut max_jolt, mut airborne, mut wall, mut max_kmh) = (0.0f32, 0.0f32, 0u64, 0u64, 0.0f32);
    let mut finished = None;
    let mut worst_at = 0.0f32;
    for t in 0..HZ * 600 {
        let i = ap.input(&sim);
        sim.step(i);
        sim.write_state(&mut st);
        let jolt = (st[9] - prev_vy).abs();
        if jolt > max_jolt {
            max_jolt = jolt;
            worst_at = st[33];
        }
        prev_vy = st[9];
        max_kmh = max_kmh.max(st[11]);
        if st[16..20].iter().sum::<f32>() < 4.0 {
            airborne += 1;
        }
        if sim.vehicle().touching_wall {
            wall += 1;
        }
        if let LapEvent::Completed { ticks, .. } = sim.last_event() {
            finished = Some((t, ticks));
        }
        if finished.is_some() {
            break;
        }
    }
    let total = sim.tick();
    eprintln!(
        "tiempo {:?} s · máx {:.0} km/h · golpe vertical máx {:.2} m/s en s={:.0} · {} ticks sin las 4 ruedas · {} ticks contra muro",
        finished.map(|f| f.1 as f32 / HZ as f32),
        max_kmh,
        max_jolt,
        worst_at,
        airborne,
        wall
    );
    assert!(finished.is_some(), "el piloto automático no llegó a la meta (s = {:.0} m)", sim.view().s);
    assert!(!sim.invalid());
    assert!(max_jolt < 0.8, "golpe vertical de {max_jolt:.2} m/s en s = {worst_at:.0} m");
    assert!((airborne as f32) < total as f32 * 0.01, "ruedas sin contacto en {airborne} de {total} ticks");
    assert!((wall as f32) < total as f32 * 0.01, "contacto con muros en {wall} de {total} ticks");
}
