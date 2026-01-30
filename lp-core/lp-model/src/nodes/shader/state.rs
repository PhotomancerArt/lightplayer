use alloc::string::String;
use serde::{Deserialize, Serialize};

/// Shader node state - runtime values
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShaderState {
    /// Actual GLSL code loaded from file
    pub glsl_code: String,
    /// Compilation/runtime errors
    pub error: Option<String>,
}
