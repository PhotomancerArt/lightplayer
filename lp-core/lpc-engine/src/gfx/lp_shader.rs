use crate::engine::error::Error;
use alloc::string::{String, ToString};
use lps_shared::LpsValueF32;

/// Backend-agnostic compile options understood by `lp-engine`.
pub struct ShaderCompileOptions {
    /// Q32 arithmetic options (saturating/wrapping add/sub/mul/div).
    pub q32_options: lps_q32::q32_options::Q32Options,
    /// Maximum semantic errors from the GLSL → LPIR front-end.
    pub max_errors: Option<usize>,
    /// GLSL frontend used before LPIR lowering.
    pub frontend: lp_shader::ShaderFrontend,
}

impl Default for ShaderCompileOptions {
    fn default() -> Self {
        Self {
            q32_options: lps_q32::q32_options::Q32Options::default(),
            max_errors: Some(20),
            frontend: default_shader_frontend(),
        }
    }
}

impl ShaderCompileOptions {
    pub fn to_compiler_config(&self) -> lpir::CompilerConfig {
        lpir::CompilerConfig {
            q32: self.q32_options,
            ..Default::default()
        }
    }
}

fn default_shader_frontend() -> lp_shader::ShaderFrontend {
    lp_shader::ShaderFrontend::default()
}

/// A compiled, runnable shader (pixel loop lives in `lp_shader::LpsPxShader::render_frame`).
pub trait LpShader: Send + Sync {
    /// Run the shader into an RGBA16 texture buffer allocated from the same graphics engine.
    fn render(
        &mut self,
        texture: &mut lp_shader::LpsTextureBuf,
        uniforms: &LpsValueF32,
    ) -> Result<(), Error>;

    /// Run the shader at caller-provided Q16.16 pixel-space points.
    fn sample_rgba16(
        &mut self,
        _points: &mut lp_shader::LpsSamplePointBuf,
        _out: &mut lp_shader::LpsSampleRgba16Buf,
        _uniforms: &LpsValueF32,
    ) -> Result<(), Error> {
        Err(Error::Other {
            message: String::from("shader backend does not support direct sampling"),
        })
    }

    fn has_render(&self) -> bool;
}

/// Compiled serial compute shader.
///
/// The engine-facing trait intentionally exposes only the shader ABI: write
/// named consumed inputs, execute `tick`, and read named produced globals.
/// Slot maps, merge behavior, and value-shape materialization are handled by
/// runtime nodes above this boundary.
pub trait LpComputeShader {
    fn tick(&mut self, inputs: &[(&str, LpsValueF32)]) -> Result<(), Error>;

    fn get_output(&mut self, path: &str) -> Result<LpsValueF32, Error>;
}

impl LpComputeShader for lp_shader::LpsComputeShader {
    fn tick(&mut self, inputs: &[(&str, LpsValueF32)]) -> Result<(), Error> {
        lp_shader::LpsComputeShader::tick(self, inputs).map_err(|e| Error::Other {
            message: String::from(e.to_string()),
        })
    }

    fn get_output(&mut self, path: &str) -> Result<LpsValueF32, Error> {
        lp_shader::LpsComputeShader::get_output(self, path).map_err(|e| Error::Other {
            message: String::from(e.to_string()),
        })
    }
}
