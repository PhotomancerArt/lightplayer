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

    /// Allocate a shader output buffer in the graphics engine's shared memory.
    /// All buffers consumed by shaders compiled from this `LpGraphics` must come from
    /// this allocator (lpvm-native JIT requires guest pointers in its own pool).
    fn alloc_output_buffer(
        &self,
        width: u32,
        height: u32,
    ) -> Result<lp_shader::LpsTextureBuf, Error>;
}
