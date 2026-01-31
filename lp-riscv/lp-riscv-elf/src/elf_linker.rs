//! ELF linker error types and utilities.

/// Errors that can occur during ELF linking.
#[derive(Debug, Clone)]
pub enum LinkerError {
    /// Parse error (invalid ELF format, missing sections, etc.)
    ParseError(String),
    /// Write error (failed to write ELF file)
    WriteError(String),
}

impl core::fmt::Display for LinkerError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            LinkerError::ParseError(msg) => write!(f, "Parse error: {msg}"),
            LinkerError::WriteError(msg) => write!(f, "Write error: {msg}"),
        }
    }
}

impl std::error::Error for LinkerError {}

// Implement From<object::Error> for convenience
impl From<object::Error> for LinkerError {
    fn from(err: object::Error) -> Self {
        LinkerError::ParseError(format!("{err}"))
    }
}

// Implement From<object::write::Error> for convenience
impl From<object::write::Error> for LinkerError {
    fn from(err: object::write::Error) -> Self {
        LinkerError::WriteError(format!("{err}"))
    }
}

/// Link a static library (placeholder - not implemented yet).
pub fn link_static_library(_objects: &[&[u8]]) -> Result<Vec<u8>, LinkerError> {
    Err(LinkerError::ParseError(
        "link_static_library not implemented".to_string(),
    ))
}
