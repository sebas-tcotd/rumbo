//! Imprime el perfil por defecto como JSON (para regenerar assets/profiles/hatch-arcade.json).
#![allow(clippy::unwrap_used, clippy::expect_used, reason = "example: a diagnostic tool where a panic is the error report")]
fn main() {
    let p = sim_core::ProfileFile { id: "hatch-arcade".into(), name: "Hatch arcade".into(), ..Default::default() };
    println!("{}", serde_json::to_string_pretty(&p).unwrap());
}
