/// Errors reported by the native compiler command.
#[derive(Debug, thiserror::Error)]
pub enum RumboError {
    #[error("{0}")]
    Usage(String),
    #[error("{0}")]
    Io(String),
    #[error("{0}")]
    Compile(String),
}

impl RumboError {
    /// Machine-readable error code.
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Usage(_) => "USAGE",
            Self::Io(_) => "IO_FAILED",
            Self::Compile(_) => "COMPILE_FAILED",
        }
    }
}

impl From<String> for RumboError {
    fn from(detail: String) -> Self {
        Self::Compile(detail)
    }
}
