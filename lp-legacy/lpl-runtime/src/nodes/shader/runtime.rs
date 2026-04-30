use alloc::{
    boxed::Box,
    format,
    string::{String, ToString},
    sync::Arc,
};
#[cfg(feature = "panic-recovery")]
use core::panic::AssertUnwindSafe;
use log;
use lp_perf::EVENT_SHADER_COMPILE;
use lpc_model::{LpPathBuf, NodeId, project::FrameId};
use lpc_engine::NodeRuntime;
use lpc_engine::error::Error;
use lpc_engine::gfx::{LpGraphics, LpShader, ShaderCompileOptions};
use lpc_engine::output::OutputProvider;
use lpc_engine::runtime::contexts::{NodeInitContext, RenderContext, TextureHandle};
use lpfs::{ChangeType, FsChange};
use lpl_model::NodeConfig;
use lpl_model::glsl_opts::{AddSubMode, DivMode, MulMode};
use lpl_model::nodes::shader::{ShaderConfig, ShaderState};
#[cfg(feature = "panic-recovery")]
use unwinding::panic::catch_unwind;

/// Default max semantic errors forwarded from the GLSL → LPIR front-end.
const SHADER_COMPILE_MAX_ERRORS: usize = 20;

fn map_model_q32_options(
    opts: &lpl_model::glsl_opts::GlslOpts,
) -> lps_q32::q32_options::Q32Options {
    lps_q32::q32_options::Q32Options {
        add_sub: match opts.add_sub {
            AddSubMode::Saturating => lps_q32::q32_options::AddSubMode::Saturating,
            AddSubMode::Wrapping => lps_q32::q32_options::AddSubMode::Wrapping,
        },
        mul: match opts.mul {
            MulMode::Saturating => lps_q32::q32_options::MulMode::Saturating,
            MulMode::Wrapping => lps_q32::q32_options::MulMode::Wrapping,
        },
        div: match opts.div {
            DivMode::Saturating => lps_q32::q32_options::DivMode::Saturating,
            DivMode::Reciprocal => lps_q32::q32_options::DivMode::Reciprocal,
        },
    }
}

/// Shader node runtime
pub struct ShaderRuntime {
    config: Option<ShaderConfig>,
    graphics: Arc<dyn LpGraphics>,
    shader: Option<Box<dyn LpShader>>,
    output_buffer: Option<lp_shader::LpsTextureBuf>,
    texture_handle: Option<TextureHandle>,
    compilation_error: Option<String>,
    pub state: ShaderState,
    node_handle: NodeId,
    render_order: i32,
}

impl ShaderRuntime {
    pub fn new(node_handle: NodeId, graphics: Arc<dyn LpGraphics>) -> Self {
        Self {
            config: None,
            graphics,
            shader: None,
            output_buffer: None,
            texture_handle: None,
            compilation_error: None,
            state: ShaderState::new(FrameId::default()),
            node_handle,
            render_order: 0,
        }
    }

    pub fn set_config(&mut self, config: ShaderConfig) {
        self.render_order = config.render_order;
        self.config = Some(config);
    }

    pub fn render_order(&self) -> i32 {
        self.render_order
    }

    pub fn get_state(&self) -> ShaderState {
        self.state.clone()
    }

    pub fn targets_texture(&self, texture_handle: TextureHandle) -> bool {
        self.texture_handle.map_or(false, |h| h == texture_handle)
    }

    pub fn texture_handle(&self) -> Option<TextureHandle> {
        self.texture_handle
    }

    pub fn get_config(&self) -> Option<&ShaderConfig> {
        self.config.as_ref()
    }

    pub fn has_compilation_error(&self) -> bool {
        self.compilation_error.is_some()
    }

    pub fn compilation_error(&self) -> Option<&str> {
        self.compilation_error.as_deref()
    }

    pub fn output_buffer(&self) -> Option<&dyn lps_shared::TextureBuffer> {
        self.output_buffer
            .as_ref()
            .map(|t| t as &dyn lps_shared::TextureBuffer)
    }

    pub fn output_buffer_mut(&mut self) -> Option<&mut lp_shader::LpsTextureBuf> {
        self.output_buffer.as_mut()
    }
}

impl NodeRuntime for ShaderRuntime {
    fn init(&mut self, ctx: &dyn NodeInitContext) -> Result<(), Error> {
        let config = self.config.clone().ok_or_else(|| Error::InvalidConfig {
            node_path: format!("shader-{}", self.node_handle.as_u32()),
            reason: alloc::string::String::from("Config not set"),
        })?;

        self.resolve_texture_handle(&config, ctx)?;
        self.sync_output_buffer_from_texture_node(ctx)?;
        let _ = self.load_and_compile_shader(&config, ctx);

        Ok(())
    }

    fn render(&mut self, ctx: &mut dyn RenderContext) -> Result<(), Error> {
        let texture_handle = self.texture_handle.ok_or_else(|| Error::Other {
            message: String::from("Texture handle not resolved"),
        })?;

        let shader = self.shader.as_mut().ok_or_else(|| Error::Other {
            message: String::from(
                "Shader is not compiled (compilation may have failed or memory was shed)",
            ),
        })?;

        if !shader.has_render() {
            return Err(Error::Other {
                message: String::from("Shader has no render() entry point"),
            });
        }

        let time = ctx.get_time();
        let buf = ctx.get_target_texture_pixels_mut(texture_handle)?;
        shader.render(buf, time)
    }

    fn shed_optional_buffers(
        &mut self,
        _output_provider: Option<&dyn OutputProvider>,
    ) -> Result<(), Error> {
        self.shader = None;
        Ok(())
    }

    fn as_any(&self) -> &dyn core::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn core::any::Any {
        self
    }

    fn update_config(
        &mut self,
        new_config: Box<dyn NodeConfig>,
        ctx: &dyn NodeInitContext,
    ) -> Result<(), Error> {
        let shader_config = new_config
            .as_any()
            .downcast_ref::<ShaderConfig>()
            .ok_or_else(|| Error::InvalidConfig {
                node_path: format!("shader-{}", self.node_handle.as_u32()),
                reason: "Config is not a ShaderConfig".to_string(),
            })?;

        let old_config = self.config.clone();
        let new_config_clone = shader_config.clone();
        self.config = Some(new_config_clone.clone());
        self.render_order = shader_config.render_order;

        let texture_changed = old_config
            .as_ref()
            .map(|old| old.texture_spec != shader_config.texture_spec)
            .unwrap_or(true);

        if texture_changed {
            let texture_handle = ctx
                .resolve_texture(&shader_config.texture_spec)
                .map_err(|e| {
                    let error_msg = format!("Failed to resolve texture: {e}");
                    self.compilation_error = Some(error_msg.clone());
                    let frame_id = FrameId::default();
                    self.state.error.set(frame_id, Some(error_msg));
                    e
                })?;
            self.texture_handle = Some(texture_handle);
        }

        let glsl_path_changed = old_config
            .as_ref()
            .map(|old| old.glsl_path != shader_config.glsl_path)
            .unwrap_or(true);

        let glsl_opts_changed = old_config
            .as_ref()
            .map(|old| old.glsl_opts != shader_config.glsl_opts)
            .unwrap_or(true);

        if glsl_path_changed || glsl_opts_changed {
            let _ = self.load_and_compile_shader(&new_config_clone, ctx);
        }

        self.sync_output_buffer_from_texture_node(ctx)?;

        Ok(())
    }

    fn handle_fs_change(
        &mut self,
        change: &FsChange,
        ctx: &dyn NodeInitContext,
    ) -> Result<(), Error> {
        let glsl_path = self
            .config
            .as_ref()
            .map(|c| c.glsl_path.clone())
            .ok_or_else(|| Error::InvalidConfig {
                node_path: format!("shader-{}", self.node_handle.as_u32()),
                reason: "Config not set".to_string(),
            })?;

        if change.path.as_str() == glsl_path.as_str() {
            match change.change_type {
                ChangeType::Create | ChangeType::Modify => {
                    let config = self.config.clone().ok_or_else(|| Error::InvalidConfig {
                        node_path: format!("shader-{}", self.node_handle.as_u32()),
                        reason: "Config not set".to_string(),
                    })?;
                    let _ = self.load_and_compile_shader(&config, ctx);
                }
                ChangeType::Delete => {
                    self.shader = None;
                    let error_msg = "GLSL file deleted".to_string();
                    self.compilation_error = Some(error_msg.clone());
                    let frame_id = FrameId::default();
                    self.state.glsl_code.set(frame_id, String::new());
                    self.state.error.set(frame_id, Some(error_msg));
                }
            }
        }

        Ok(())
    }
}

impl ShaderRuntime {
    fn sync_output_buffer_from_texture_node(
        &mut self,
        ctx: &dyn NodeInitContext,
    ) -> Result<(), Error> {
        let texture_handle = self.texture_handle.ok_or_else(|| Error::Other {
            message: String::from("Texture handle not resolved"),
        })?;
        let owner = ctx.texture_output_buffer_owner(texture_handle, self.node_handle);
        if owner != self.node_handle {
            self.output_buffer = None;
            return Ok(());
        }
        let cfg = ctx.get_texture_config(texture_handle)?;
        let need_alloc = match &self.output_buffer {
            None => true,
            Some(buf) => buf.width() != cfg.width || buf.height() != cfg.height,
        };
        if need_alloc {
            let buf = self
                .graphics
                .alloc_output_buffer(cfg.width, cfg.height)
                .map_err(|e| Error::InvalidConfig {
                    node_path: format!("shader-{}", self.node_handle.as_u32()),
                    reason: format!("Failed to allocate shader output buffer: {e}"),
                })?;
            self.output_buffer = Some(buf);
        }
        Ok(())
    }

    fn resolve_texture_handle(
        &mut self,
        config: &ShaderConfig,
        ctx: &dyn NodeInitContext,
    ) -> Result<(), Error> {
        let texture_handle = ctx.resolve_texture(&config.texture_spec).map_err(|e| {
            let error_msg = format!("Failed to resolve texture: {e}");
            self.compilation_error = Some(error_msg.clone());
            let frame_id = FrameId::default();
            self.state.error.set(frame_id, Some(error_msg));
            e
        })?;
        self.texture_handle = Some(texture_handle);
        Ok(())
    }

    fn load_glsl_source(
        &mut self,
        config: &ShaderConfig,
        ctx: &dyn NodeInitContext,
    ) -> Result<String, Error> {
        let fs = ctx.get_node_fs();
        let glsl_path = &config.glsl_path;
        let glsl_path_abs = if glsl_path.is_absolute() {
            glsl_path.clone()
        } else {
            LpPathBuf::from(format!("/{}", glsl_path.as_str()))
        };
        let source_bytes = fs
            .read_file(glsl_path_abs.as_path())
            .map_err(|e| Error::Io {
                path: glsl_path.as_str().to_string(),
                details: format!("Failed to read GLSL file: {e:?}"),
            })?;

        alloc::string::String::from_utf8(source_bytes).map_err(|e| Error::Parse {
            file: glsl_path.as_str().to_string(),
            error: format!("Invalid UTF-8 in GLSL file: {e}"),
        })
    }

    fn compile_shader(&mut self, glsl_source: &str) -> Result<(), Error> {
        lp_perf::emit_begin!(EVENT_SHADER_COMPILE);
        let result = self.compile_shader_inner(glsl_source);
        lp_perf::emit_end!(EVENT_SHADER_COMPILE);
        result
    }

    fn compile_shader_inner(&mut self, glsl_source: &str) -> Result<(), Error> {
        log::info!(
            "Shader {} compilation starting ({} bytes)",
            self.node_handle.as_u32(),
            glsl_source.len()
        );
        if log::log_enabled!(log::Level::Trace) {
            let preview = if glsl_source.len() > 120 {
                format!(
                    "{}... ({} bytes total)",
                    &glsl_source[..120],
                    glsl_source.len()
                )
            } else {
                glsl_source.to_string()
            };
            log::trace!("ShaderRuntime::compile_shader: GLSL source:\n{preview}");
        }

        let q32_options = self
            .config
            .as_ref()
            .map(|c| map_model_q32_options(&c.glsl_opts))
            .unwrap_or_default();

        log::info!(
            "Shader {} q32 options: add_sub={:?}, mul={:?}, div={:?}",
            self.node_handle.as_u32(),
            q32_options.add_sub,
            q32_options.mul,
            q32_options.div,
        );

        let compile_opts = ShaderCompileOptions {
            q32_options,
            max_errors: Some(SHADER_COMPILE_MAX_ERRORS),
        };

        self.shader = None;
        self.compilation_error = None;

        #[cfg(feature = "panic-recovery")]
        let compile_result: Result<Box<dyn LpShader>, String> =
            match catch_unwind(AssertUnwindSafe(|| {
                self.graphics.compile_shader(glsl_source, &compile_opts)
            })) {
                Ok(inner) => inner.map_err(|e| format!("{e}")),
                Err(_) => Err(String::from("OOM during shader compilation")),
            };
        #[cfg(not(feature = "panic-recovery"))]
        let compile_result: Result<Box<dyn LpShader>, String> = self
            .graphics
            .compile_shader(glsl_source, &compile_opts)
            .map_err(|e| format!("{e}"));

        match compile_result {
            Ok(shader) => {
                self.shader = Some(shader);
                self.compilation_error = None;
                let frame_id = FrameId::default();
                self.state.error.set(frame_id, None);
                Ok(())
            }
            Err(e) => {
                self.compilation_error = Some(e.clone());
                self.shader = None;
                let frame_id = FrameId::default();
                self.state.error.set(frame_id, Some(e.clone()));
                log::warn!(
                    "Shader {} compilation failed: {}",
                    self.node_handle.as_u32(),
                    e
                );
                Err(Error::InvalidConfig {
                    node_path: format!("shader-{}", self.node_handle.as_u32()),
                    reason: format!("GLSL compilation failed: {e}"),
                })
            }
        }
    }

    fn load_and_compile_shader(
        &mut self,
        config: &ShaderConfig,
        ctx: &dyn NodeInitContext,
    ) -> Result<(), Error> {
        let glsl_source = self.load_glsl_source(config, ctx)?;
        let frame_id = FrameId::default();
        self.state.glsl_code.set(frame_id, glsl_source.clone());

        let start_ms = ctx.now_ms();
        let result = self.compile_shader(glsl_source.as_str());
        if result.is_ok() {
            if let (Some(start), Some(end)) = (start_ms, ctx.now_ms()) {
                let elapsed_ms = end.saturating_sub(start);
                log::info!(
                    "Shader {} compiled in {}ms",
                    self.node_handle.as_u32(),
                    elapsed_ms
                );
            } else {
                log::info!("Shader {} compiled", self.node_handle.as_u32());
            }
        }
        result?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shader_runtime_creation() {
        let handle = lpc_model::NodeId::new(0);
        let graphics: Arc<dyn LpGraphics> = Arc::new(lpc_engine::Graphics::new());
        let runtime = ShaderRuntime::new(handle, graphics);
        let _boxed: alloc::boxed::Box<dyn NodeRuntime> = alloc::boxed::Box::new(runtime);
    }
}
