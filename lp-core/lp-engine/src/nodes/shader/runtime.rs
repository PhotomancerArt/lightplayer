use crate::error::Error;
use crate::nodes::{NodeConfig, NodeRuntime};
use crate::output::OutputProvider;
use crate::runtime::contexts::{NodeInitContext, RenderContext, TextureHandle};
use alloc::{
    boxed::Box,
    format,
    string::{String, ToString},
};
#[cfg(all(feature = "std", feature = "panic-recovery"))]
use core::panic::AssertUnwindSafe;
use log;
#[cfg(feature = "std")]
use lp_model::glsl_opts::{AddSubMode, DivMode, MulMode};
use lp_model::{
    LpPathBuf, NodeHandle,
    nodes::shader::{ShaderConfig, ShaderState},
    project::FrameId,
};
use lp_shared::fs::fs_event::FsChange;
#[cfg(all(feature = "std", feature = "panic-recovery"))]
use unwinding::panic::catch_unwind;

#[cfg(feature = "std")]
use lpir_cranelift::{CompileOptions, FloatMode, JitModule, MemoryStrategy, Q32Options, jit};

/// Default max semantic errors forwarded to the GLSL front-end (matches `lp-glsl-frontend`).
#[cfg(feature = "std")]
const SHADER_COMPILE_MAX_ERRORS: usize = 20;

#[cfg(feature = "std")]
fn map_add_sub(m: AddSubMode) -> lpir_cranelift::AddSubMode {
    match m {
        AddSubMode::Saturating => lpir_cranelift::AddSubMode::Saturating,
        AddSubMode::Wrapping => lpir_cranelift::AddSubMode::Wrapping,
    }
}

#[cfg(feature = "std")]
fn map_mul(m: MulMode) -> lpir_cranelift::MulMode {
    match m {
        MulMode::Saturating => lpir_cranelift::MulMode::Saturating,
        MulMode::Wrapping => lpir_cranelift::MulMode::Wrapping,
    }
}

#[cfg(feature = "std")]
fn map_div(m: DivMode) -> lpir_cranelift::DivMode {
    match m {
        DivMode::Saturating => lpir_cranelift::DivMode::Saturating,
        DivMode::Reciprocal => lpir_cranelift::DivMode::Reciprocal,
    }
}

/// Shader node runtime
pub struct ShaderRuntime {
    config: Option<ShaderConfig>,
    #[cfg(feature = "std")]
    jit_module: Option<JitModule>,
    #[cfg(feature = "std")]
    direct_call: Option<lpir_cranelift::DirectCall>,
    texture_handle: Option<TextureHandle>,
    compilation_error: Option<String>,
    pub state: ShaderState,
    node_handle: NodeHandle,
    render_order: i32,
}

impl ShaderRuntime {
    pub fn new(node_handle: NodeHandle) -> Self {
        Self {
            config: None,
            #[cfg(feature = "std")]
            jit_module: None,
            #[cfg(feature = "std")]
            direct_call: None,
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
}

impl NodeRuntime for ShaderRuntime {
    fn init(&mut self, ctx: &dyn NodeInitContext) -> Result<(), Error> {
        let config = self.config.clone().ok_or_else(|| Error::InvalidConfig {
            node_path: format!("shader-{}", self.node_handle.as_i32()),
            reason: alloc::string::String::from("Config not set"),
        })?;

        self.resolve_texture_handle(&config, ctx)?;
        let _ = self.load_and_compile_shader(&config, ctx);

        Ok(())
    }

    fn render(&mut self, ctx: &mut dyn RenderContext) -> Result<(), Error> {
        #[cfg(not(feature = "std"))]
        {
            let _ = ctx;
            return Err(Error::Other {
                message: String::from("Shader JIT requires `lp-engine` `std` feature"),
            });
        }

        #[cfg(feature = "std")]
        {
            let texture_handle = self.texture_handle.ok_or_else(|| Error::Other {
                message: String::from("Texture handle not resolved"),
            })?;

            let dc = self.direct_call.as_ref().ok_or_else(|| Error::Other {
                message: String::from(
                    "Shader has no direct call for `main` (compilation may have omitted fast path)",
                ),
            })?;

            let time = ctx.get_time();
            let texture = ctx.get_texture_mut(texture_handle)?;
            let width = texture.width();
            let height = texture.height();

            Self::render_direct_call(dc, width, height, time, texture)?;
            Ok(())
        }
    }

    fn shed_optional_buffers(
        &mut self,
        _output_provider: Option<&dyn OutputProvider>,
    ) -> Result<(), Error> {
        #[cfg(feature = "std")]
        {
            self.jit_module = None;
            self.direct_call = None;
        }
        self.state
            .glsl_code
            .set(lp_model::project::FrameId::default(), String::new());
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
                node_path: format!("shader-{}", self.node_handle.as_i32()),
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

        if glsl_path_changed {
            let _ = self.load_and_compile_shader(&new_config_clone, ctx);
        }

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
                node_path: format!("shader-{}", self.node_handle.as_i32()),
                reason: "Config not set".to_string(),
            })?;

        if change.path.as_str() == glsl_path.as_str() {
            match change.change_type {
                lp_shared::fs::fs_event::ChangeType::Create
                | lp_shared::fs::fs_event::ChangeType::Modify => {
                    let config = self.config.clone().ok_or_else(|| Error::InvalidConfig {
                        node_path: format!("shader-{}", self.node_handle.as_i32()),
                        reason: "Config not set".to_string(),
                    })?;
                    let _ = self.load_and_compile_shader(&config, ctx);
                }
                lp_shared::fs::fs_event::ChangeType::Delete => {
                    #[cfg(feature = "std")]
                    {
                        self.jit_module = None;
                        self.direct_call = None;
                    }
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
    #[cfg(feature = "std")]
    fn render_direct_call(
        dc: &lpir_cranelift::DirectCall,
        width: u32,
        height: u32,
        time: f32,
        texture: &mut lp_shared::Texture,
    ) -> Result<(), Error> {
        const Q32_SCALE: i32 = 65536;
        let time_q32 = (time * 65536.0 + 0.5) as i32;
        let output_size_q32 = [(width as i32) * Q32_SCALE, (height as i32) * Q32_SCALE];

        for y in 0..height {
            for x in 0..width {
                let frag_coord_q32 = [(x as i32) * Q32_SCALE, (y as i32) * Q32_SCALE];
                let args = [
                    frag_coord_q32[0],
                    frag_coord_q32[1],
                    output_size_q32[0],
                    output_size_q32[1],
                    time_q32,
                ];
                let mut rgba_q32 = [0i32; 4];
                unsafe {
                    dc.call_i32_buf(&args, &mut rgba_q32)
                        .map_err(|e| Error::Other {
                            message: format!("Shader direct call failed: {e}"),
                        })?;
                }

                let clamp_q32 = |v: i32| -> i32 {
                    if v < 0 {
                        0
                    } else if v > Q32_SCALE {
                        Q32_SCALE
                    } else {
                        v
                    }
                };

                let r = ((clamp_q32(rgba_q32[0]) as i64 * 65535) / Q32_SCALE as i64) as u16;
                let g = ((clamp_q32(rgba_q32[1]) as i64 * 65535) / Q32_SCALE as i64) as u16;
                let b = ((clamp_q32(rgba_q32[2]) as i64 * 65535) / Q32_SCALE as i64) as u16;
                let a = ((clamp_q32(rgba_q32[3]) as i64 * 65535) / Q32_SCALE as i64) as u16;

                texture.set_pixel_u16(x, y, [r, g, b, a]);
            }
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

    #[cfg(feature = "std")]
    fn compile_shader(&mut self, glsl_source: &str) -> Result<(), Error> {
        log::info!(
            "Shader {} compilation starting ({} bytes)",
            self.node_handle.as_i32(),
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
            .map(|c| Q32Options {
                add_sub: map_add_sub(c.glsl_opts.add_sub),
                mul: map_mul(c.glsl_opts.mul),
                div: map_div(c.glsl_opts.div),
            })
            .unwrap_or_default();

        let options = CompileOptions {
            float_mode: FloatMode::Q32,
            q32_options,
            // Match previous `GlslOptions` with `std`: host JIT used `memory_optimized == false`.
            memory_strategy: MemoryStrategy::Default,
            max_errors: Some(SHADER_COMPILE_MAX_ERRORS),
        };

        self.jit_module = None;
        self.direct_call = None;

        #[cfg(feature = "panic-recovery")]
        let compile_result: Result<JitModule, String> =
            match catch_unwind(AssertUnwindSafe(|| jit(glsl_source, &options))) {
                Ok(inner) => inner.map_err(|e| format!("{e}")),
                Err(_) => Err(String::from("OOM during shader compilation")),
            };
        #[cfg(not(feature = "panic-recovery"))]
        let compile_result: Result<JitModule, String> =
            jit(glsl_source, &options).map_err(|e| format!("{e}"));

        match compile_result {
            Ok(module) => {
                let dc = module.direct_call("main");
                self.direct_call = dc;
                self.jit_module = Some(module);
                self.compilation_error = None;
                let frame_id = FrameId::default();
                self.state.error.set(frame_id, None);
                Ok(())
            }
            Err(e) => {
                self.compilation_error = Some(e.clone());
                self.jit_module = None;
                self.direct_call = None;
                let frame_id = FrameId::default();
                self.state.error.set(frame_id, Some(e.clone()));
                log::warn!(
                    "Shader {} compilation failed: {}",
                    self.node_handle.as_i32(),
                    e
                );
                Err(Error::InvalidConfig {
                    node_path: format!("shader-{}", self.node_handle.as_i32()),
                    reason: format!("GLSL compilation failed: {e}"),
                })
            }
        }
    }

    #[cfg(not(feature = "std"))]
    fn compile_shader(&mut self, _glsl_source: &str) -> Result<(), Error> {
        Err(Error::Other {
            message: String::from("Shader JIT requires `lp-engine` `std` feature"),
        })
    }

    fn load_and_compile_shader(
        &mut self,
        config: &ShaderConfig,
        ctx: &dyn NodeInitContext,
    ) -> Result<(), Error> {
        let glsl_source = self.load_glsl_source(config, ctx)?;
        let start_ms = ctx.now_ms();
        let result = self.compile_shader(glsl_source.as_str());
        if result.is_ok() {
            if let (Some(start), Some(end)) = (start_ms, ctx.now_ms()) {
                let elapsed_ms = end.saturating_sub(start);
                log::info!(
                    "Shader {} compiled in {}ms",
                    self.node_handle.as_i32(),
                    elapsed_ms
                );
            } else {
                log::info!("Shader {} compiled", self.node_handle.as_i32());
            }
        }
        result?;
        let frame_id = FrameId::default();
        self.state.glsl_code.set(frame_id, glsl_source);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shader_runtime_creation() {
        let handle = lp_model::NodeHandle::new(0);
        let runtime = ShaderRuntime::new(handle);
        let _boxed: alloc::boxed::Box<dyn NodeRuntime> = alloc::boxed::Box::new(runtime);
    }
}
