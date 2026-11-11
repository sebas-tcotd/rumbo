//! Depuración: trayectoria del piloto automático (CSV: t, x, z, km/h, dirección, freno, s).
//! Uso: cargo run -p sim-core --release --example trace -- <pista.json> <desde s> <hasta s>
#![allow(clippy::disallowed_methods, reason = "diagnostic tool outside the core: it reads the track file from disk")]
#![allow(clippy::unwrap_used, clippy::expect_used, reason = "example: a diagnostic tool where a panic is the error report")]
use sim_core::autopilot::Autopilot;
use sim_core::{ProfileFile, Simulation, TrackDef, STATE_LEN};
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let def = TrackDef::from_json(&std::fs::read_to_string(&a[1]).unwrap()).unwrap();
    let (t0, t1): (f32, f32) = (a[2].parse().unwrap(), a[3].parse().unwrap());
    let mut sim = Simulation::new(&def, &ProfileFile::default(), 7).unwrap();
    let ap = Autopilot::default();
    let mut st = [0.0f32; STATE_LEN];
    println!("t,x,z,kmh,steer,brake,s,wall");
    for t in 0..(t1 * 120.0) as u64 {
        let i = ap.input(&sim);
        sim.step(i);
        sim.write_state(&mut st);
        let tt = t as f32 / 120.0;
        if tt >= t0 && t % 6 == 0 {
            println!("{tt:.2},{:.2},{:.2},{:.1},{},{},{:.1},{}", st[1], st[3], st[11], i.steer, i.brake, st[33], sim.vehicle().touching_wall as u8);
        }
    }
}
