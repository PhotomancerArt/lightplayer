use crate::engine::error::Error;
use alloc::string::String;

/// Backend-agnostic compile options understood by `lp-engine`.
pub struct ShaderCompileOptions {
    /// Q32 arithmetic options (saturating/wrapping add/sub/mul/div).
    pub q32_options: lps_q32::q32_options::Q32Options,
    /// Maximum semantic errors from the GLSL → LPIR front-end.
    pub max_errors: Option<usize>,
}

impl Default for ShaderCompileOptions {
    fn default() -> Self {
        Self {
            q32_options: lps_q32::q32_options::Q32Options::default(),
            max_errors: Some(20),
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

/// A compiled, runnable shader (pixel loop lives in `lp_shader::LpsPxShader::render_frame`).
pub trait LpShader: Send + Sync {
    /// Run the shader into an RGBA16 texture buffer allocated from the same graphics engine.
    fn render(&mut self, texture: &mut lp_shader::LpsTextureBuf, time: f32) -> Result<(), Error>;

    /// Run the shader at caller-provided Q16.16 pixel-space points.
    fn sample_rgba16(
        &mut self,
        _points: &mut lp_shader::LpsSamplePointBuf,
        _out: &mut lp_shader::LpsSampleRgba16Buf,
        _output_width: u32,
        _output_height: u32,
        _time: f32,
    ) -> Result<(), Error> {
        Err(Error::Other {
            message: String::from("shader backend does not support direct sampling"),
        })
    }

    fn has_render(&self) -> bool;
}
