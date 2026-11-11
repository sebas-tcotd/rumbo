//! S4 · Replays dorados: el set que se verifica en nativo, en Node y en cada navegador.
//!
//! Cada caso corre en nativo y guarda su replay (entradas + hashes de control cada 5 s) en
//! `tests/golden/<pista>.<caso>.json`. La pista se busca por su id en `assets/tracks/`.
//! El arnés de CI (`ci/determinism`) re-simula los mismos archivos con el WASM y exige los
//! mismos hashes: si coinciden en todas partes, la simulación es determinista entre
//! plataformas. Para regenerarlos (al cambiar la física): `UPDATE_GOLDEN=1 cargo test --test golden`.
#![allow(clippy::disallowed_methods, reason = "test harness outside the core: it reads fixtures from disk (golden.rs also honours an env var to record them)")]
#![allow(clippy::unwrap_used, clippy::expect_used, reason = "test code: a panic is the failure report")]
mod common;
use common::{scripted_input, HZ};
use sim_core::autopilot::Autopilot;
use sim_core::input::{FLAG_HANDBRAKE, FLAG_RESET, FLAG_RESTART};
use sim_core::{InputFrame, ProfileFile, ReplayManifest, Simulation, TrackDef};
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

fn record(case: &Case) -> ReplayManifest {
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

#[test]
fn golden_replays_match() {
    let dir = root().join("crates/sim-core/tests/golden");
    std::fs::create_dir_all(&dir).unwrap();
    let update = std::env::var("UPDATE_GOLDEN").is_ok();
    let mut created = Vec::new();
    for case in CASES {
        let path = dir.join(format!("{}.{}.json", case.track, case.name));
        if update || !path.exists() {
            std::fs::write(&path, serde_json::to_string(&record(case)).unwrap()).unwrap();
            created.push(path.display().to_string());
        }
        let manifest = ReplayManifest::from_json(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(manifest.track_id, case.track);
        let report = sim_core::verify_replay(&manifest, &track(case.track)).unwrap();
        assert!(
            report.ok,
            "{}.{}: {} (esperado {}, obtenido {})",
            case.track, case.name, report.message, report.expected_final, report.actual_final
        );
        eprintln!("{}.{}: {} ticks, final {}", case.track, case.name, report.ticks, report.actual_final);
    }
    if !created.is_empty() && !update {
        panic!("faltaban replays dorados; se generaron: {created:?}. Vuelve a correr las pruebas.");
    }
}

/// Grabar dos veces el mismo caso da el mismo replay, byte a byte.
#[test]
fn recording_is_reproducible() {
    for case in CASES.iter().filter(|c| matches!(c.driver, Driver::Adversarial)) {
        let a = serde_json::to_string(&record(case)).unwrap();
        let b = serde_json::to_string(&record(case)).unwrap();
        assert_eq!(a, b, "{}.{}", case.track, case.name);
    }
}
