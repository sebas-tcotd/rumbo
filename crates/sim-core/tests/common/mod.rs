#![allow(dead_code)]
use sim_core::autopilot::Autopilot;
use sim_core::input::{FLAG_HANDBRAKE, FLAG_RESET};
use sim_core::{InputFrame, ProfileFile, Simulation, TrackDef};

pub const HZ: u64 = 120;

pub fn track() -> TrackDef {
    TrackDef::from_json(include_str!("../../../../assets/tracks/test-loop.json")).unwrap()
}

pub fn profile() -> ProfileFile {
    ProfileFile::default()
}

pub fn sim() -> Simulation {
    Simulation::new(&track(), &profile(), 7).unwrap()
}

pub fn run_autopilot(sim: &mut Simulation, ticks: u64) {
    let ap = Autopilot::default();
    for _ in 0..ticks {
        let i = ap.input(sim);
        sim.step(i);
    }
}

/// Entradas "humanas" con teclado: dirección digital, toques de freno, freno de mano y resets.
pub fn scripted_input(t: u64) -> InputFrame {
    let sec = t / HZ;
    let phase = t % (HZ * 8);
    let steer = match phase / (HZ / 2) {
        3 | 4 => 127,
        9 | 10 => -127,
        _ => 0,
    };
    let brake = if (HZ * 3 / 2..HZ * 3 / 2 + 15).contains(&phase) { 255 } else { 0 };
    let mut flags = 0;
    if (HZ * 5..HZ * 5 + 30).contains(&phase) {
        flags |= FLAG_HANDBRAKE;
    }
    if sec % 20 == 19 && t % HZ == 0 {
        flags |= FLAG_RESET;
    }
    InputFrame { steer, throttle: if brake > 0 { 0 } else { 230 }, brake, flags }
}
