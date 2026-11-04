//! S3: circuito urbano con cruces (San Borja, Las Artes Norte).
//!
//! Usa `assets/tracks/san-borja-las-artes.json`, generada con `track-compiler track --network`.
//! El piloto automático da vueltas completas pasando por cruces en T y de cuatro brazos,
//! girando y siguiendo derecho: sin saltos, sin ruedas en el aire y sin tocar muros.
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
    let t = def.compile().unwrap();
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
