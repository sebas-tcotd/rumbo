//! Compara las entradas al derrape: freno + dirección contra freno de mano + dirección.
//! Uso: cargo run -p sim-core --release --example drift_entry
#![allow(clippy::unwrap_used, clippy::expect_used, reason = "example: a diagnostic tool where a panic is the error report")]
use sim_core::input::FLAG_HANDBRAKE;
use sim_core::{InputFrame, ProfileFile, Simulation, TrackDef, STATE_LEN};

fn run(label: &str, entry: InputFrame) {
    let track = TrackDef::from_json(include_str!("../../../assets/tracks/test-loop.json")).unwrap();
    let mut s = Simulation::new(&track, &ProfileFile::default(), 1).unwrap();
    let mut st = [0.0f32; STATE_LEN];
    loop {
        s.step(InputFrame { throttle: 255, ..Default::default() });
        s.write_state(&mut st);
        if st[11] > 110.0 { break; }
    }
    let (mut max_slip, mut drift_ticks, mut min_v) = (0.0f32, 0, f32::MAX);
    for t in 0..120 {
        let i = if t < 10 { InputFrame { steer: 127, ..entry } } else { InputFrame { steer: 127, throttle: 255, ..Default::default() } };
        s.step(i);
        s.write_state(&mut st);
        max_slip = max_slip.max(st[12].abs());
        min_v = min_v.min(st[11]);
        if st[15] > 0.5 { drift_ticks += 1; }
    }
    println!("{label:<26} ángulo máx {max_slip:>5.1}°  derrape {:>4.2} s  vel. mínima {min_v:>5.1} km/h", drift_ticks as f32 / 120.0);
}

fn main() {
    run("freno + dirección", InputFrame { brake: 255, ..Default::default() });
    run("freno de mano + dirección", InputFrame { flags: FLAG_HANDBRAKE, ..Default::default() });
}
