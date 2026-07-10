//! Backend error type shared by all [`crate::LpGraphics`] implementations.

use alloc::string::String;

/// Failure raised by a graphics backend.
///
/// Each variant carries a complete human-readable message; [`core::fmt::Display`]
/// prints only that message (variants classify, they do not add prefixes), so
/// engine-side wrappers stay in charge of user-facing composition
/// (e.g. `"shader compile: {error}"`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GfxError {
    /// Shader compilation failed (parse, lower, codegen, or validation).
    Compile(String),
    /// Resource allocation failed (texture or sample buffers).
    Alloc(String),
    /// Shader execution / rendering / sampling failed.
    Render(String),
    /// The backend cannot service the request at all: unsupported capability
    /// (e.g. an explicit [`crate::ShaderSemantics`] tier this backend does not
    /// implement, compute shaders on a render-only backend) or a handle that
    /// belongs to a different backend.
    Backend(String),
}

impl core::fmt::Display for GfxError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Compile(message)
            | Self::Alloc(message)
            | Self::Render(message)
            | Self::Backend(message) => f.write_str(message),
        }
    }
}

impl core::error::Error for GfxError {}
