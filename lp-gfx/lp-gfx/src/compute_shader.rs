//! The [`LpComputeShader`] trait: a compiled serial compute shader.

use lps_shared::LpsValueF32;

use crate::gfx_error::GfxError;
use crate::shader::ShaderCompileStats;

/// Compiled serial compute shader.
///
/// The engine-facing trait intentionally exposes only the shader ABI: write
/// named consumed inputs, execute `tick`, and read named produced globals.
/// Slot maps, merge behavior, and value-shape materialization are handled by
/// runtime nodes above this boundary. Compute shaders stay on the CPU tier
/// permanently (see `docs/adr/2026-07-09-preview-fidelity-tiers.md`).
pub trait LpComputeShader {
    fn tick(&mut self, inputs: &[(&str, LpsValueF32)]) -> Result<(), GfxError>;

    fn get_output(&mut self, path: &str) -> Result<LpsValueF32, GfxError>;

    fn compile_stats(&self) -> Option<ShaderCompileStats> {
        None
    }
}
