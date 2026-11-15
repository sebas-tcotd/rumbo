use wasm_bindgen::JsValue;

/// Stable errors at the WASM boundary. JavaScript receives a string so the host
/// can distinguish a rejected operation from a WebAssembly trap.
#[derive(Debug, thiserror::Error)]
pub enum RumboError {
    #[error("{0}")]
    InvalidTrack(String),
    #[error("{0}")]
    InvalidProfile(String),
    #[error("{0}")]
    InvalidReplay(String),
    #[error("{0}")]
    ExportFailed(String),
    #[error("{0}")]
    SchemaFailed(String),
}

impl RumboError {
    /// Stable codes in parity order.
    pub const CODES: &[&str] = &[
        "INVALID_TRACK",
        "INVALID_PROFILE",
        "INVALID_REPLAY",
        "EXPORT_FAILED",
        "SCHEMA_FAILED",
    ];

    /// Machine-readable error code.
    pub const fn code(&self) -> &'static str {
        match self {
            Self::InvalidTrack(_) => Self::CODES[0],
            Self::InvalidProfile(_) => Self::CODES[1],
            Self::InvalidReplay(_) => Self::CODES[2],
            Self::ExportFailed(_) => Self::CODES[3],
            Self::SchemaFailed(_) => Self::CODES[4],
        }
    }
}

impl From<RumboError> for JsValue {
    fn from(error: RumboError) -> Self {
        Self::from_str(&format!("{}: {error}", error.code()))
    }
}
