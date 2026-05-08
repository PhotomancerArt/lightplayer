//! Lazy shader-backed render product.

use alloc::boxed::Box;
use alloc::format;
use alloc::string::String;

use lp_shader::TextureBuffer;
use lpc_model::nodes::shader::ShaderDef;
use lpc_model::{AddSubMode, DivMode, GlslOpts, MulMode};

use crate::gfx::{LpGraphics, LpShader, ShaderCompileOptions};

use crate::render_product::{
    RenderProduct, RenderProductError, RenderSampleBatch, RenderSampleBatchResult,
    RenderTextureRequest, TextureRenderProduct,
};

/// Default max semantic errors forwarded from the GLSL to LPIR front end.
const SHADER_COMPILE_MAX_ERRORS: usize = 20;

/// Render product that compiles a shader when first materialized and then reuses it.
pub struct ShaderRenderProduct {
    config: ShaderDef,
    glsl_source: String,
    shader: Option<Box<dyn LpShader>>,
    compilation_error: Option<String>,
}

impl ShaderRenderProduct {
    pub fn new(config: ShaderDef, glsl_source: String) -> Self {
        Self {
            config,
            glsl_source,
            shader: None,
            compilation_error: None,
        }
    }

    pub fn compilation_error(&self) -> Option<&str> {
        self.compilation_error.as_deref()
    }

    fn ensure_compiled(&mut self, graphics: &dyn LpGraphics) -> Result<(), RenderProductError> {
        if self.shader.is_some() {
            return Ok(());
        }

        log::info!(
            "[shader-product] compilation starting ({} bytes)",
            self.glsl_source.len()
        );
        lp_perf::emit_begin!(lp_perf::EVENT_SHADER_COMPILE);
        self.compilation_error = None;
        let compile_opts = ShaderCompileOptions {
            q32_options: map_model_q32_options(&self.config.glsl_opts),
            max_errors: Some(SHADER_COMPILE_MAX_ERRORS),
        };

        #[cfg(feature = "panic-recovery")]
        let compile_result: Result<Box<dyn LpShader>, String> = {
            use core::panic::AssertUnwindSafe;
            use unwinding::panic::catch_unwind;
            match catch_unwind(AssertUnwindSafe(|| {
                graphics.compile_shader(self.glsl_source.as_str(), &compile_opts)
            })) {
                Ok(inner) => inner.map_err(|e| format!("{e}")),
                Err(_) => Err(String::from("OOM during shader compilation")),
            }
        };
        #[cfg(not(feature = "panic-recovery"))]
        let compile_result: Result<Box<dyn LpShader>, String> = graphics
            .compile_shader(self.glsl_source.as_str(), &compile_opts)
            .map_err(|e| format!("{e}"));
        lp_perf::emit_end!(lp_perf::EVENT_SHADER_COMPILE);

        match compile_result {
            Ok(shader) => {
                self.shader = Some(shader);
                log::info!("[shader-product] compilation succeeded");
                Ok(())
            }
            Err(error) => {
                self.compilation_error = Some(error.clone());
                self.shader = None;
                log::warn!("[shader-product] compilation failed: {error}");
                Err(RenderProductError::RenderFailed {
                    message: format!("shader compile: {error}"),
                })
            }
        }
    }
}

impl RenderProduct for ShaderRenderProduct {
    fn sample_batch(
        &self,
        _request: &RenderSampleBatch,
    ) -> Result<RenderSampleBatchResult, RenderProductError> {
        Err(RenderProductError::NotRenderable)
    }

    fn render_texture(
        &mut self,
        request: &RenderTextureRequest,
        graphics: Option<&dyn LpGraphics>,
    ) -> Result<TextureRenderProduct, RenderProductError> {
        let graphics = graphics.ok_or_else(|| RenderProductError::RenderFailed {
            message: String::from("missing graphics backend"),
        })?;
        self.ensure_compiled(graphics)?;
        let shader = self
            .shader
            .as_mut()
            .ok_or_else(|| RenderProductError::RenderFailed {
                message: String::from("shader missing after compile"),
            })?;
        if !shader.has_render() {
            return Err(RenderProductError::RenderFailed {
                message: String::from("compiled shader has no render() entry"),
            });
        }

        let mut texture = graphics
            .alloc_output_buffer(request.width, request.height)
            .map_err(|e| RenderProductError::RenderFailed {
                message: format!("alloc_output_buffer: {e}"),
            })?;
        if texture.format() != request.format {
            return Err(RenderProductError::RenderFailed {
                message: format!(
                    "graphics allocated {:?}, requested {:?}",
                    texture.format(),
                    request.format
                ),
            });
        }
        shader
            .render(&mut texture, request.time_seconds)
            .map_err(|e| RenderProductError::RenderFailed {
                message: format!("shader render: {e}"),
            })?;

        TextureRenderProduct::new(
            texture.width(),
            texture.height(),
            texture.format(),
            texture.data().to_vec(),
        )
        .map_err(|e| RenderProductError::RenderFailed {
            message: format!("texture product: {e}"),
        })
    }

    fn as_any(&self) -> &dyn core::any::Any {
        self
    }
}

fn map_model_q32_options(opts: &GlslOpts) -> lps_q32::q32_options::Q32Options {
    lps_q32::q32_options::Q32Options {
        add_sub: match opts.add_sub.value() {
            AddSubMode::Saturating => lps_q32::q32_options::AddSubMode::Saturating,
            AddSubMode::Wrapping => lps_q32::q32_options::AddSubMode::Wrapping,
        },
        mul: match opts.mul.value() {
            MulMode::Saturating => lps_q32::q32_options::MulMode::Saturating,
            MulMode::Wrapping => lps_q32::q32_options::MulMode::Wrapping,
        },
        div: match opts.div.value() {
            DivMode::Saturating => lps_q32::q32_options::DivMode::Saturating,
            DivMode::Reciprocal => lps_q32::q32_options::DivMode::Reciprocal,
        },
    }
}
