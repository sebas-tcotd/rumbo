//! S4 · Replays dorados: el set que se verifica en nativo, en Node y en cada navegador.
//!
//! Cada caso es un `rumbo-replay/2` (entradas a 60 Hz + hash de control cada 10 s + hash final) en
//! `tests/golden/<pista>.<caso>.bin`. La pista sale del nombre del archivo y el perfil es
//! `ProfileFile::default()` (el replay lleva `LEGACY_VEHICLE`). El arnés de CI (`ci/determinism`)
//! re-simula los mismos archivos con el WASM y exige los mismos hashes por tick.
//!
//! Desde la 0.10 los dorados son **flujos de entrada fijos**. Nacieron de la migración de
//! `tests/golden-v1/` (`examples/replay_v1_to_v2.rs`). Para regenerarlos al cambiar la física, en un
//! commit `physics:` que sube `PHYSICS_VERSION`: `UPDATE_GOLDEN=1 cargo test --release --test golden`.
//! Eso **re-simula las mismas entradas** y reescribe controles y hash final; nunca vuelve a grabar desde
//! los conductores, así la comparación entre versiones de física es de lo mismo con lo mismo.
#![allow(
    clippy::disallowed_methods,
    reason = "test harness outside the core: it reads fixtures from disk (golden.rs also honours an env var to record them)"
)]
#![allow(clippy::unwrap_used, clippy::expect_used, reason = "test code: a panic is the failure report")]
mod common;
use common::{scripted_input, HZ};
use sim_core::autopilot::Autopilot;
use sim_core::input::{FLAG_HANDBRAKE, FLAG_RESET, FLAG_RESTART};
use sim_core::replay::{migrate_v1, v1_frames};
use sim_core::{InputFrame, ProfileFile, ReplayManifest, ReplayV2, Simulation, TrackDef};
use std::path::PathBuf;

#[derive(Clone, Copy)]
enum Driver {
    Autopilot,
    /// Teclado: dirección digital, toques de freno, freno de mano y resets.
    Scripted,
    /// Casos adversos: a fondo contra los muros, volante de tope a tope, freno de mano,
    /// resets y reinicios, con una secuencia pseudoaleatoria fija.
    Adversarial,
}

struct Case {
    track: &'static str,
    name: &'static str,
    driver: Driver,
    seconds: u64,
    seed: u64,
}

const CASES: &[Case] = &[
    Case { track: "test-loop", name: "autopilot", driver: Driver::Autopilot, seconds: 60, seed: 7 },
    Case { track: "test-loop", name: "scripted", driver: Driver::Scripted, seconds: 60, seed: 11 },
    Case { track: "test-loop", name: "adversarial", driver: Driver::Adversarial, seconds: 45, seed: 13 },
    Case { track: "costa-verde-sprint", name: "autopilot", driver: Driver::Autopilot, seconds: 90, seed: 17 },
    Case { track: "costa-verde-sprint", name: "adversarial", driver: Driver::Adversarial, seconds: 45, seed: 19 },
    Case { track: "san-borja-las-artes", name: "autopilot", driver: Driver::Autopilot, seconds: 60, seed: 23 },
    Case { track: "san-borja-las-artes", name: "scripted", driver: Driver::Scripted, seconds: 60, seed: 29 },
    Case { track: "san-borja-las-artes", name: "adversarial", driver: Driver::Adversarial, seconds: 60, seed: 31 },
];

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn track(id: &str) -> TrackDef {
    TrackDef::from_json(&std::fs::read_to_string(root().join(format!("assets/tracks/{id}.json"))).unwrap()).unwrap()
}

/// Generador congruencial fijo: misma secuencia en cualquier plataforma.
struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> u32 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (self.0 >> 33) as u32
    }
}

/// Records a case at 60 Hz: the driver is asked every tick and the simulation keeps the first tick of
/// each pair (D7), as the worker does.
fn record(case: &Case) -> ReplayV2 {
    let mut sim = Simulation::new(&track(case.track), &ProfileFile::default(), case.seed).unwrap();
    let ap = Autopilot::default();
    let mut rng = Lcg(case.seed);
    let (mut steer, mut hold) = (0i8, 0u32);
    for t in 0..HZ * case.seconds {
        let input = match case.driver {
            Driver::Autopilot => ap.input(&sim),
            Driver::Scripted => scripted_input(t),
            Driver::Adversarial => {
                if hold == 0 {
                    steer = [127i8, -127, 0, 64, -64][(rng.next() % 5) as usize];
                    hold = 30 + rng.next() % 150;
                }
                hold -= 1;
                let r = rng.next() % 1000;
                let mut flags = 0;
                if r < 15 {
                    flags |= FLAG_HANDBRAKE;
                }
                if t % 1700 == 1699 {
                    flags |= FLAG_RESET;
                }
                if t % 4100 == 4099 {
                    flags |= FLAG_RESTART;
                }
                let brake = if r > 985 { 255 } else { 0 };
                InputFrame { steer, throttle: if brake > 0 { 0 } else { 255 }, brake, flags }
            }
        };
        sim.step(input);
    }
    sim.replay()
}

fn golden_path(case: &Case) -> PathBuf {
    root().join(format!("crates/sim-core/tests/golden/{}.{}.bin", case.track, case.name))
}

/// Current physics over the same 60 Hz frames: what `UPDATE_GOLDEN` writes.
fn resimulate(replay: &ReplayV2, track: &TrackDef) -> ReplayV2 {
    let mut sim = Simulation::new(track, &ProfileFile::default(), replay.header.seed).unwrap();
    for frame in replay.tick_frames() {
        sim.step(frame);
    }
    sim.replay()
}

#[test]
fn golden_replays_match() {
    let update = std::env::var("UPDATE_GOLDEN").is_ok();
    let dir = root().join("crates/sim-core/tests/golden");
    let mut names: Vec<String> =
        std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name().into_string().unwrap()).collect();
    names.sort();
    let mut expected: Vec<String> = CASES.iter().map(|c| format!("{}.{}.bin", c.track, c.name)).collect();
    expected.sort();
    assert_eq!(names, expected, "el set de dorados es fijo; agregar o quitar uno es un cambio de P1");
    for case in CASES {
        let path = golden_path(case);
        let replay = ReplayV2::decode(&std::fs::read(&path).unwrap()).unwrap();
        let def = track(case.track);
        if update {
            std::fs::write(&path, resimulate(&replay, &def).encode().unwrap()).unwrap();
            continue;
        }
        assert_eq!(replay.header.seed, case.seed, "{}.{}", case.track, case.name);
        let report = sim_core::verify_replay(&replay, &def, &ProfileFile::default()).unwrap();
        assert!(
            report.ok,
            "{}.{}: {} (esperado {}, obtenido {})",
            case.track, case.name, report.message, report.expected_final, report.actual_final
        );
        eprintln!("{}.{}: {} ticks, final {}", case.track, case.name, report.ticks, report.actual_final);
    }
    assert!(!update, "dorados re-simulados con la física actual; vuelve a correr las pruebas sin UPDATE_GOLDEN");
}

/// Grabar dos veces el mismo caso a 60 Hz da el mismo replay, byte a byte.
#[test]
fn recording_is_reproducible() {
    for case in CASES.iter().filter(|c| matches!(c.driver, Driver::Adversarial)) {
        let a = record(case).encode().unwrap();
        let b = record(case).encode().unwrap();
        assert_eq!(a, b, "{}.{}", case.track, case.name);
    }
}

/// The autopilot goldens are the autopilot driven at 60 Hz (closed loop) from the case's seed: the
/// migration re-drove them instead of converting a controller's output (story 0.10).
#[test]
fn autopilot_goldens_are_the_60_hz_autopilot() {
    for case in CASES.iter().filter(|c| matches!(c.driver, Driver::Autopilot)) {
        let golden = std::fs::read(golden_path(case)).unwrap();
        assert_eq!(record(case).encode().unwrap(), golden, "{}.{}", case.track, case.name);
    }
}

/// The open-loop goldens are the deterministic conversion of their v1 originals (`tests/golden-v1`).
#[test]
fn converted_goldens_match_a_deterministic_migration() {
    for case in CASES.iter().filter(|c| !matches!(c.driver, Driver::Autopilot)) {
        let v1_path = root().join(format!("crates/sim-core/tests/golden-v1/{}.{}.json", case.track, case.name));
        let v1 = ReplayManifest::from_json(&std::fs::read_to_string(v1_path).unwrap()).unwrap();
        let a = migrate_v1(&v1, &track(case.track)).unwrap().encode().unwrap();
        let b = migrate_v1(&v1, &track(case.track)).unwrap().encode().unwrap();
        assert_eq!(a, b);
        assert_eq!(a, std::fs::read(golden_path(case)).unwrap(), "{}.{}", case.track, case.name);
    }
}

/// v1 → 60 Hz: axes and handbrake from the first tick of the pair, events from both ticks.
#[test]
fn v1_frames_keep_every_event() {
    let v1 = ReplayManifest::from_json(
        &std::fs::read_to_string(root().join("crates/sim-core/tests/golden-v1/test-loop.adversarial.json")).unwrap(),
    )
    .unwrap();
    let ticks: Vec<InputFrame> =
        v1.inputs.iter().flat_map(|r| std::iter::repeat_n(InputFrame::unpack(r[1]), r[0] as usize)).collect();
    let frames = v1_frames(&v1);
    assert_eq!(frames.len() as u64, ReplayV2::frames_for(v1.ticks));
    let events = |f: &InputFrame| f.flags & (FLAG_RESET | FLAG_RESTART);
    assert_eq!(
        ticks.iter().filter(|f| events(f) != 0).count(),
        frames.iter().filter(|f| events(f) != 0).count(),
        "this golden never has events on both ticks of a pair, so the counts match"
    );
    for (k, f) in frames.iter().enumerate() {
        assert_eq!((f.steer, f.throttle, f.brake, f.flags & FLAG_HANDBRAKE), {
            let t = ticks[2 * k];
            (t.steer, t.throttle, t.brake, t.flags & FLAG_HANDBRAKE)
        });
    }
}
