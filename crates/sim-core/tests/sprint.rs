//! Pistas abiertas (sprint): cronometraje de salida a llegada, vuelta a la grilla y muros finales.
use sim_core::autopilot::Autopilot;
use sim_core::input::FLAG_RESTART;
use sim_core::race::LapEvent;
use sim_core::{InputFrame, ProfileFile, Simulation, TrackDef};

const HZ: u64 = 120;

fn s_track() -> TrackDef {
    // Una S suave de unos 700 m con algo de pendiente.
    let mut pts = Vec::new();
    for i in 0..=35 {
        let t = i as f32 / 35.0;
        let x = t * 700.0;
        let z = 60.0 * (t * std::f32::consts::TAU).sin();
        let y = 4.0 * (t * std::f32::consts::PI).sin();
        pts.push(format!(r#"{{"x":{x},"z":{z},"y":{y}}}"#));
    }
    let json = format!(
        r#"{{"id":"s-sprint","name":"S de prueba","closed":false,"checkpoints":[0.5],"points":[{}]}}"#,
        pts.join(",")
    );
    TrackDef::from_json(&json).unwrap()
}

#[test]
fn open_track_compiles_with_lines_inside_the_ends() {
    let t = s_track().compile().unwrap();
    assert!(!t.closed);
    assert!(t.length > 650.0 && t.length < 900.0, "longitud {}", t.length);
    assert_eq!(t.checkpoints.len(), 3, "salida, intermedio y llegada: {:?}", t.checkpoints);
    assert!(t.start_s > 0.0 && t.finish_s < t.length && t.start_s < t.finish_s);
    // Las muestras no dan la vuelta: la última queda en el extremo final.
    let last = t.samples.last().unwrap();
    assert!((last.s - t.length).abs() < 0.01);
    assert!(last.pos.x > 690.0);
}

#[test]
fn autopilot_finishes_the_sprint_and_stops_before_the_end_wall() {
    let mut sim = Simulation::new(&s_track(), &ProfileFile::default(), 3).unwrap();
    let ap = Autopilot::default();
    let mut started = false;
    let mut finished = None;
    for t in 0..HZ * 90 {
        let i = ap.input(&sim);
        sim.step(i);
        match sim.last_event() {
            LapEvent::Started => started = true,
            LapEvent::Completed { ticks, .. } => finished = Some((t, ticks)),
            _ => {}
        }
        if let Some((tf, _)) = finished {
            if t > tf + HZ * 8 {
                break;
            }
        }
    }
    assert!(started, "nunca cruzó la salida");
    let (_, ticks) = finished.expect("nunca llegó a la meta");
    assert!(ticks > HZ * 10, "tiempo implausible: {ticks} ticks");
    let v = sim.view();
    assert!(v.vel.length() < 1.0, "debería haberse detenido: {:.1} m/s", v.vel.length());
    assert!(v.s <= sim.track().length, "pasó el final de la pista");
    assert!(!sim.vehicle().touching_wall, "terminó contra un muro");
}

#[test]
fn restart_returns_to_the_grid_and_discards_the_attempt() {
    let mut sim = Simulation::new(&s_track(), &ProfileFile::default(), 3).unwrap();
    let ap = Autopilot::default();
    for _ in 0..HZ * 8 {
        let i = ap.input(&sim);
        sim.step(i);
    }
    assert!(sim.timer().started, "a los 8 s debería estar en carrera");
    sim.step(InputFrame { flags: FLAG_RESTART, ..Default::default() });
    assert!(!sim.timer().started);
    assert!(sim.view().s < sim.track().start_s, "no volvió antes de la salida");
    assert!(sim.view().vel.length() < 0.5);
}

#[test]
fn end_wall_stops_a_car_at_full_throttle() {
    let mut sim = Simulation::new(&s_track(), &ProfileFile::default(), 3).unwrap();
    let ap = Autopilot::default();
    let mut touched = false;
    for _ in 0..HZ * 80 {
        let mut i = ap.input(&sim);
        if sim.view().s > sim.track().finish_s - 40.0 {
            i = InputFrame { steer: i.steer, throttle: 255, brake: 0, flags: 0 };
        }
        sim.step(i);
        touched |= sim.vehicle().touching_wall;
    }
    assert!(touched, "el auto debería llegar al muro final");
    assert!(sim.view().s <= sim.track().length + 0.5, "atravesó el muro final");
    assert!(!sim.invalid());
}

#[test]
fn sprint_replays_exactly() {
    let mut sim = Simulation::new(&s_track(), &ProfileFile::default(), 3).unwrap();
    let ap = Autopilot::default();
    for t in 0..HZ * 40 {
        let mut i = ap.input(&sim);
        if t == HZ * 20 {
            i.flags |= FLAG_RESTART;
        }
        sim.step(i);
    }
    let report = sim.replay().verify(&s_track()).unwrap();
    assert!(report.ok, "{}", report.message);
}
