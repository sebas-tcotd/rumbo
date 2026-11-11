//! Depuración: dónde toca muros el piloto automático en una pista.
//! Uso: cargo run -p sim-core --release --example wallprobe -- <pista.json> [segundos]
#![allow(clippy::disallowed_methods, reason = "diagnostic tool outside the core: it reads the track file from disk")]
#![allow(clippy::unwrap_used, clippy::expect_used, reason = "example: a diagnostic tool where a panic is the error report")]
use sim_core::autopilot::Autopilot;
use sim_core::{ProfileFile, Simulation, TrackDef, STATE_LEN};
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let def = TrackDef::from_json(&std::fs::read_to_string(&a[1]).unwrap()).unwrap();
    let secs: u64 = a.get(2).map_or(100, |s| s.parse().unwrap());
    let mut sim = Simulation::new(&def, &ProfileFile::default(), 7).unwrap();
    let ap = Autopilot::default();
    let mut st = [0.0f32; STATE_LEN];
    let mut was = false;
    for t in 0..secs * 120 {
        sim.step(ap.input(&sim));
        sim.write_state(&mut st);
        let w = sim.vehicle().touching_wall;
        if w && !was {
            println!("t={:.2}s pos=(x {:.1}, y {:.1}, z {:.1}, norte {:.1}) s={:.0} {:.0} km/h", t as f32 / 120.0, st[1], st[2], st[3], -st[3], st[33], st[11]);
        }
        was = w;
    }
}
