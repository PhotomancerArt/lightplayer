//! Compiled serial compute shader running on an LPVM engine.

use alloc::string::ToString;

use lp_gfx::{GfxError, LpComputeShader, ShaderCompileStats};
use lp_shader::LpsComputeShader;
use lps_shared::LpsValueF32;

/// [`LpComputeShader`] over a compiled [`LpsComputeShader`].
pub struct LpvmComputeShader {
    inner: LpsComputeShader,
}

impl LpvmComputeShader {
    pub(crate) fn new(inner: LpsComputeShader) -> Self {
        Self { inner }
    }
}

impl LpComputeShader for LpvmComputeShader {
    fn tick(&mut self, inputs: &[(&str, LpsValueF32)]) -> Result<(), GfxError> {
        self.inner
            .tick(inputs)
            .map_err(|e| GfxError::Render(e.to_string()))
    }

    fn get_output(&mut self, path: &str) -> Result<LpsValueF32, GfxError> {
        self.inner
            .get_output(path)
            .map_err(|e| GfxError::Render(e.to_string()))
    }

    fn compile_stats(&self) -> Option<ShaderCompileStats> {
        Some(self.inner.compile_stats())
    }
}
