use crate::error::Error;
use crate::nodes::{NodeConfig, NodeRuntime};
use crate::runtime::contexts::{NodeInitContext, RenderContext, TextureHandle};
use alloc::{
    boxed::Box,
    format,
    string::{String, ToString},
};
use log;
use lp_glsl_compiler::{DecimalFormat, GlslExecutable, GlslOptions, RunMode, glsl_jit};
use lp_model::{
    LpPathBuf, NodeHandle,
    nodes::shader::{ShaderConfig, ShaderState},
};
use lp_shared::fs::fs_event::FsChange;

/// Shader node runtime
pub struct ShaderRuntime {
    config: Option<ShaderConfig>,
    glsl_source: Option<String>, // Stored for state extraction
    executable: Option<Box<dyn GlslExecutable + Send + Sync>>, // Compiled shader (must be Send + Sync for NodeRuntime)
    texture_handle: Option<TextureHandle>,                     // Resolved texture handle
    compilation_error: Option<String>,                         // Compilation error if any
    node_handle: NodeHandle,
    render_order: i32, // Render order (from config)
}

impl ShaderRuntime {
    pub fn new(node_handle: NodeHandle) -> Self {
        Self {
            config: None,
            glsl_source: None,
            executable: None,
            texture_handle: None,
            compilation_error: None,
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
        ShaderState {
            glsl_code: self.glsl_source.clone().unwrap_or_default(),
            error: self.compilation_error.clone(),
        }
    }

    /// Check if this shader targets the given texture handle
    pub fn targets_texture(&self, texture_handle: TextureHandle) -> bool {
        self.texture_handle.map_or(false, |h| h == texture_handle)
    }

    /// Get the texture handle this shader targets
    pub fn texture_handle(&self) -> Option<TextureHandle> {
        self.texture_handle
    }

    /// Get the shader config (for state extraction)
    pub fn get_config(&self) -> Option<&ShaderConfig> {
        self.config.as_ref()
    }

    /// Check if shader has compilation error
    pub fn has_compilation_error(&self) -> bool {
        self.compilation_error.is_some()
    }

    /// Get compilation error message
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

        // Resolve texture handle
        self.resolve_texture_handle(&config, ctx)?;

        // Load and compile shader
        // Don't fail on compilation errors - store them and return Ok()
        // The caller will check compilation_error and update status accordingly
        let _ = self.load_and_compile_shader(&config, ctx);

        Ok(())
    }

    fn render(&mut self, ctx: &mut dyn RenderContext) -> Result<(), Error> {
        log::trace!(
            "ShaderRuntime::render: Starting render for shader {}",
            self.node_handle.as_i32()
        );
        let texture_handle = self.texture_handle.ok_or_else(|| Error::Other {
            message: String::from("Texture handle not resolved"),
        })?;

        let executable = self.executable.as_mut().ok_or_else(|| Error::Other {
            message: String::from("Shader not compiled"),
        })?;

        // Get time before mutable borrow
        let time = ctx.get_time();

        // Get mutable texture access
        let texture = ctx.get_texture_mut(texture_handle)?;

        let width = texture.width();
        let height = texture.height();
        let output_size = [width as f32, height as f32];

        // Execute shader for each pixel
        for y in 0..height {
            for x in 0..width {
                let frag_coord = [x as f32, y as f32];

                // Call shader main function
                // Signature: vec4 main(vec2 fragCoord, vec2 outputSize, float time)
                let result = executable
                    .call_vec(
                        "main",
                        &[
                            lp_glsl_compiler::GlslValue::Vec2(frag_coord),
                            lp_glsl_compiler::GlslValue::Vec2(output_size),
                            lp_glsl_compiler::GlslValue::F32(time),
                        ],
                        4,
                    )
                    .map_err(|e| Error::Other {
                        message: format!("Shader execution failed: {e}"),
                    })?;

                // Extract RGBA from vec4 result
                // Result is Vec<f32> with 4 elements [r, g, b, a] in [0, 1] range
                if result.len() != 4 {
                    return Err(Error::Other {
                        message: format!(
                            "Shader main() must return vec4, got {} components",
                            result.len()
                        ),
                    });
                }

                // Convert from [0, 1] to [0, 255] and clamp
                let rgba = [
                    (result[0].clamp(0.0, 1.0) * 255.0) as u8,
                    (result[1].clamp(0.0, 1.0) * 255.0) as u8,
                    (result[2].clamp(0.0, 1.0) * 255.0) as u8,
                    (result[3].clamp(0.0, 1.0) * 255.0) as u8,
                ];

                // Write to texture
                texture.set_pixel(x, y, rgba);
            }
        }

        log::trace!(
            "ShaderRuntime::render: Completed render for shader {}",
            self.node_handle.as_i32()
        );
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
        // Downcast to ShaderConfig
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

        // If texture_spec changed, re-resolve texture handle
        let texture_changed = old_config
            .as_ref()
            .map(|old| old.texture_spec != shader_config.texture_spec)
            .unwrap_or(true);

        if texture_changed {
            let texture_handle = ctx
                .resolve_texture(&shader_config.texture_spec)
                .map_err(|e| {
                    self.compilation_error = Some(format!("Failed to resolve texture: {e}"));
                    e
                })?;
            self.texture_handle = Some(texture_handle);
        }

        // If glsl_path changed, reload and recompile
        let glsl_path_changed = old_config
            .as_ref()
            .map(|old| old.glsl_path != shader_config.glsl_path)
            .unwrap_or(true);

        if glsl_path_changed {
            self.load_and_compile_shader(&new_config_clone, ctx)?;
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

        // Check if this change affects the shader's GLSL file
        if change.path.as_str() == glsl_path.as_str() {
            match change.change_type {
                lp_shared::fs::fs_event::ChangeType::Create
                | lp_shared::fs::fs_event::ChangeType::Modify => {
                    let config = self.config.clone().ok_or_else(|| Error::InvalidConfig {
                        node_path: format!("shader-{}", self.node_handle.as_i32()),
                        reason: "Config not set".to_string(),
                    })?;
                    // Don't fail on compilation errors - store them and return Ok()
                    // The caller will check compilation_error and update status accordingly
                    let _ = self.load_and_compile_shader(&config, ctx);
                }
                lp_shared::fs::fs_event::ChangeType::Delete => {
                    // Clear shader executable
                    self.executable = None;
                    self.glsl_source = None;
                    self.compilation_error = Some("GLSL file deleted".to_string());
                }
            }
        }

        Ok(())
    }
}

impl ShaderRuntime {
    /// Resolve texture handle from config
    fn resolve_texture_handle(
        &mut self,
        config: &ShaderConfig,
        ctx: &dyn NodeInitContext,
    ) -> Result<(), Error> {
        let texture_handle = ctx.resolve_texture(&config.texture_spec).map_err(|e| {
            self.compilation_error = Some(format!("Failed to resolve texture: {e}"));
            e
        })?;
        self.texture_handle = Some(texture_handle);
        Ok(())
    }

    /// Load GLSL source from filesystem
    fn load_glsl_source(
        &mut self,
        config: &ShaderConfig,
        ctx: &dyn NodeInitContext,
    ) -> Result<String, Error> {
        let fs = ctx.get_node_fs();
        let glsl_path = &config.glsl_path;
        // Make path absolute if relative (chrooted filesystem requires absolute paths)
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

        let glsl_source =
            alloc::string::String::from_utf8(source_bytes).map_err(|e| Error::Parse {
                file: glsl_path.as_str().to_string(),
                error: format!("Invalid UTF-8 in GLSL file: {e}"),
            })?;

        // Store source for state extraction
        self.glsl_source = Some(glsl_source.clone());

        Ok(glsl_source)
    }

    /// Compile GLSL source into executable
    fn compile_shader(&mut self, glsl_source: &str) -> Result<(), Error> {
        log::debug!(
            "ShaderRuntime::compile_shader: Compiling shader {} ({} bytes)",
            self.node_handle.as_i32(),
            glsl_source.len()
        );
        log::trace!("ShaderRuntime::compile_shader: GLSL source:\n{glsl_source}");

        let options = GlslOptions {
            run_mode: RunMode::HostJit,
            decimal_format: DecimalFormat::Q32,
        };

        match glsl_jit(glsl_source, options) {
            Ok(executable) => {
                // Cast to add Send + Sync bounds (GlslJitModule is safe to send/sync)
                // The function pointers are stable and don't change after compilation
                let executable_with_bounds: Box<dyn GlslExecutable + Send + Sync> =
                    unsafe { core::mem::transmute(executable) };
                self.executable = Some(executable_with_bounds);
                self.compilation_error = None;
                log::debug!(
                    "ShaderRuntime::compile_shader: Shader {} compiled successfully",
                    self.node_handle.as_i32()
                );
                Ok(())
            }
            Err(e) => {
                self.compilation_error = Some(format!("{e}"));
                self.executable = None;
                log::warn!(
                    "ShaderRuntime::compile_shader: Shader {} compilation failed: {}",
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

    /// Load and compile shader from filesystem
    fn load_and_compile_shader(
        &mut self,
        config: &ShaderConfig,
        ctx: &dyn NodeInitContext,
    ) -> Result<(), Error> {
        let glsl_source = self.load_glsl_source(config, ctx)?;
        self.compile_shader(&glsl_source)
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
