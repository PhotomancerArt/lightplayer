use crate::engine::error::Error;
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

    /// Release a transient shader output buffer allocated by [`Self::alloc_output_buffer`].
    ///
    /// Some backends use bump allocation and cannot reclaim individual buffers,
    /// but native embedded backends return this memory to the heap. Product
    /// materialization paths must call this for short-lived render targets.
    fn free_output_buffer(&self, buffer: lp_shader::LpsTextureBuf);

    fn alloc_sample_points(&self, count: u32) -> Result<lp_shader::LpsSamplePointBuf, Error>;

    fn alloc_sample_rgba16(&self, count: u32) -> Result<lp_shader::LpsSampleRgba16Buf, Error>;

    fn free_sample_points(&self, buffer: lp_shader::LpsSamplePointBuf);

    fn free_sample_rgba16(&self, buffer: lp_shader::LpsSampleRgba16Buf);
}
