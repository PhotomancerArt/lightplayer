use crate::error::Error;
use lp_shared::Texture;

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

/// A compiled, runnable shader (pixel loop lives here to avoid per-pixel dynamic dispatch).
pub trait LpShader: Send + Sync {
    /// Run the shader `render` entry point into an RGBA16 texture.
    fn render(&mut self, texture: &mut Texture, time: f32) -> Result<(), Error>;

    fn has_render(&self) -> bool;
}
