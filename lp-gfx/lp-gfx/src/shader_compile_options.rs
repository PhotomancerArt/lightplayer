//! Backend-agnostic shader compile options.

use crate::shader_semantics::ShaderSemantics;

/// Backend-agnostic compile options understood by every [`crate::LpGraphics`].
pub struct ShaderCompileOptions {
    /// Numeric semantics tier this shader must be compiled with.
    ///
    /// Explicit so no backend can silently ignore `q32_options`: a backend
    /// that does not implement the requested tier must fail compilation.
    pub semantics: ShaderSemantics,
    /// Q32 arithmetic options (saturating/wrapping add/sub/mul/div).
    /// Only meaningful for [`ShaderSemantics::Q32`].
    pub q32_options: lps_q32::q32_options::Q32Options,
    /// Maximum semantic errors from the GLSL → LPIR front-end.
    pub max_errors: Option<usize>,
    /// GLSL frontend used before LPIR lowering.
    pub frontend: lp_shader::ShaderFrontend,
}

impl Default for ShaderCompileOptions {
    fn default() -> Self {
        Self {
            semantics: ShaderSemantics::default(),
            q32_options: lps_q32::q32_options::Q32Options::default(),
            max_errors: Some(20),
            frontend: lp_shader::ShaderFrontend::default(),
        }
    }
}

impl ShaderCompileOptions {
    /// LPIR compiler configuration for the Q32 tier.
    pub fn to_compiler_config(&self) -> lpir::CompilerConfig {
        lpir::CompilerConfig {
            q32: self.q32_options,
            ..Default::default()
        }
    }
}
