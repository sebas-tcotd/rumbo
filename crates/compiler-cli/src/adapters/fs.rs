pub fn read(path: &str) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|e| format!("no se pudo leer {path}: {e}"))
}

pub fn write(path: &str, v: &serde_json::Value, pretty: bool) -> Result<(), String> {
    if let Some(dir) = std::path::Path::new(path).parent() {
        if !dir.as_os_str().is_empty() {
            std::fs::create_dir_all(dir)
                .map_err(|e| format!("no se pudo crear {}: {e}", dir.display()))?;
        }
    }
    let s = if pretty {
        serde_json::to_string_pretty(v)
    } else {
        serde_json::to_string(v)
    }
    .map_err(|e| e.to_string())?;
    std::fs::write(path, s).map_err(|e| format!("no se pudo escribir {path}: {e}"))
}

pub struct FileZoneWriter<'a>(pub &'a str);
impl track_compiler_core::ports::ZoneWriter for FileZoneWriter<'_> {
    fn publish(&self, track: &serde_json::Value) -> Result<(), String> {
        write(self.0, track, true)
    }
}
