//! S4 native reference (story 0.9): the per-tick trace of every golden replay, computed natively with the
//! same `harness::trace` the WASM test binary exports. CI writes it once and every WASM environment
//! (Node and the five browsers) is compared against it, tick by tick.
//!
//! Usage: `cargo run --release --locked -p sim-wasm --features harness --example trace_reference -- <out.json>`
//! Output: `{ "physicsVersion", "cases": [{ "name", "ticks", "trace", "summary" }] }`, cases sorted by file
//! name. The goldens are `rumbo-replay/2` (`<track>.<case>.bin`): the track id is the file name prefix and
//! `summary` is `sim_wasm::summary` (header, ticks, checks, final hash), so JavaScript never decodes postcard.
//! Never linked into the browser binary.
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

fn case(root: &Path, path: &Path) -> Result<Value, String> {
    let name = path.file_stem().and_then(|s| s.to_str()).ok_or("nombre de archivo inválido")?.to_string();
    let replay = std::fs::read(path).map_err(|e| format!("{name}: {e}"))?;
    let track_id = name.split('.').next().ok_or_else(|| format!("{name}: sin pista"))?;
    let track = std::fs::read_to_string(root.join(format!("assets/tracks/{track_id}.json")))
        .map_err(|e| format!("{name}: pista {track_id}: {e}"))?;
    let t = sim_wasm::harness::trace(&track, &replay).map_err(|e| format!("{name}: {} {e}", e.code()))?;
    let decoded = sim_core::ReplayV2::decode(&replay).map_err(|e| format!("{name}: {e}"))?;
    Ok(json!({ "name": name, "ticks": t.ticks, "trace": t.hex, "summary": sim_wasm::summary(&decoded) }))
}

fn run(out: &Path) -> Result<usize, String> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let dir = root.join("crates/sim-core/tests/golden");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "bin"))
        .collect();
    files.sort();
    let cases = files.iter().map(|p| case(&root, p)).collect::<Result<Vec<_>, _>>()?;
    let n = cases.len();
    let body = json!({ "physicsVersion": sim_core::PHYSICS_VERSION, "cases": cases });
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    std::fs::write(out, body.to_string()).map_err(|e| format!("{}: {e}", out.display()))?;
    Ok(n)
}

fn main() -> ExitCode {
    let Some(out) = std::env::args().nth(1) else {
        eprintln!("uso: trace_reference <out.json>");
        return ExitCode::FAILURE;
    };
    match run(Path::new(&out)) {
        Ok(n) => {
            println!("referencia nativa por tick: {n} replays → {out}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}
