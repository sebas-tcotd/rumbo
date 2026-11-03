//! Sonda de diagnóstico: imprime telemetría de escenarios básicos.
//! Uso: cargo run -p sim-core --release --example probe
use sim_core::autopilot::Autopilot;
use sim_core::race::LapEvent;
use sim_core::{InputFrame, ProfileFile, Simulation, TrackDef, STATE_LEN};

fn main() {
    let track = TrackDef::from_json(include_str!("../../../assets/tracks/test-loop.json")).unwrap();
    let profile = ProfileFile::default();
    let mut st = [0.0f32; STATE_LEN];

    // 1. Reposo
    let mut sim = Simulation::new(&track, &profile, 1).unwrap();
    for _ in 0..240 { sim.step(InputFrame::default()); }
    sim.write_state(&mut st);
    println!("reposo 2 s: y={:.3} v={:.3} km/h comp=[{:.3} {:.3} {:.3} {:.3}] contacto={:?}",
        st[2], st[11], st[20], st[21], st[22], st[23], &st[16..20]);
    println!("pista: {:.0} m, {} muestras, hash {}", sim.track().length, sim.track().samples.len(), sim.track_hash_hex());

    // 2. Aceleración 0-100 en recta
    let mut sim = Simulation::new(&track, &profile, 1).unwrap();
    for _ in 0..60 { sim.step(InputFrame::default()); }
    let full = InputFrame { throttle: 255, ..Default::default() };
    let mut t100 = None;
    for i in 0..1200 {
        sim.step(full);
        sim.write_state(&mut st);
        if t100.is_none() && st[11] >= 100.0 { t100 = Some(i as f32 / 120.0); }
    }
    println!("0-100: {:?} s, a los 10 s: {:.1} km/h, slip {:.2}°, yaw {:.3}", t100, st[11], st[12], st[35]);

    // 3. Piloto automático, 4 minutos
    let mut sim = Simulation::new(&track, &profile, 1).unwrap();
    let ap = Autopilot::default();
    let mut maxspd = 0.0f32; let mut max_slip = 0.0f32; let mut min_y_body = f32::MAX;
    for i in 0..(120 * 240) {
        let inp = ap.input(&sim);
        sim.step(inp);
        sim.write_state(&mut st);
        maxspd = maxspd.max(st[11]); max_slip = max_slip.max(st[12].abs());
        let contacts: f32 = st[16..20].iter().sum();
        if contacts < 4.0 && i % 120 == 0 { /* aire */ }
        min_y_body = min_y_body.min(st[2]);
        match sim.last_event() {
            LapEvent::Completed { ticks, best } => println!("  vuelta {:.2} s (mejor: {best}) tick {}", ticks as f32 / 120.0, i),
            LapEvent::Invalidated => println!("  vuelta anulada tick {i}"),
            LapEvent::Started => println!("  cronómetro iniciado tick {i}"),
            _ => {}
        }
        if i % (120 * 10) == 0 {
            println!("  t={:>3}s s={:>6.0} v={:>5.1} slip={:>5.1} steer={:>5.1} cp={} inval={}", i / 120, st[33], st[11], st[12], st[13], st[32], st[39]);
        }
    }
    drift_probe();
    println!("autopiloto: vmax {:.0} km/h, slip máx {:.1}°, vueltas {:?}, inválido {}", maxspd, max_slip, sim.timer().completed, sim.invalid());
}

#[allow(dead_code)]
pub fn drift_probe() {
    let track = TrackDef::from_json(include_str!("../../../assets/tracks/test-loop.json")).unwrap();
    let mut st = [0.0f32; STATE_LEN];
    let mut sim = Simulation::new(&track, &ProfileFile::default(), 1).unwrap();
    // acelerar en la recta hasta ~110 km/h
    loop {
        sim.step(InputFrame { throttle: 255, ..Default::default() });
        sim.write_state(&mut st);
        if st[11] > 110.0 { break; }
    }
    // toque de freno con volante a la derecha, luego acelerador sostenido
    for i in 0..360 {
        let brake = if i < 10 { 255 } else { 0 };
        let steer = if i < 200 { 127 } else { 0 };
        sim.step(InputFrame { steer, throttle: if i >= 10 { 255 } else { 0 }, brake, flags: 0 });
        sim.write_state(&mut st);
        if i % 20 == 0 {
            println!("  drift t={:.2} v={:>5.1} slip={:>6.1} drift={} yaw={:>5.2} steer={:>5.1} y={:.2} cont={:?}", i as f32/120.0, st[11], st[12], st[15], st[35], st[13], st[2], &st[16..20]);
        }
    }
    // choque: volante a fondo a la derecha a máxima velocidad
    let mut sim = Simulation::new(&track, &ProfileFile::default(), 1).unwrap();
    for _ in 0..600 { sim.step(InputFrame { throttle: 255, ..Default::default() }); }
    sim.write_state(&mut st);
    println!("pre-choque v={:.1}", st[11]);
    for i in 0..480 {
        sim.step(InputFrame { throttle: 255, steer: if i < 150 { 127 } else { 0 }, ..Default::default() });
        sim.write_state(&mut st);
        if i % 30 == 0 { println!("  choque t={:.2} v={:>6.1} y={:.2} slip={:>6.1} cont={:?} inval={}", i as f32/120.0, st[11], st[2], st[12], &st[16..20], st[39]); }
    }
}
