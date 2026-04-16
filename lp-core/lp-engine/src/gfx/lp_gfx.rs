use crate::error::Error;
use crate::gfx::lp_shader::{LpShader, ShaderCompileOptions};
use alloc::boxed::Box;

/// Compiles GLSL and produces runnable shaders for this process.
pub trait LpGraphics: Send + Sync {
    fn compile_shader(
        &self,
        source: &str,
        options: &ShaderCompileOptions,
    ) -> Result<Box<dyn LpShader>, Error>;

    /// Human-readable label for logs (e.g. `cranelift`, `wasm`).
    fn backend_name(&self) -> &'static str {
        "unknown"
    }
}
