//! Shared compile options for JIT and object emission.

use lpir::FloatMode;

/// Options for LPIR → Cranelift compilation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CompileOptions {
    pub float_mode: FloatMode,
}

impl Default for CompileOptions {
    fn default() -> Self {
        Self {
            float_mode: FloatMode::Q32,
        }
    }
}
