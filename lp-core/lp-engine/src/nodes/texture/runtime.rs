use crate::error::Error;
use crate::nodes::{NodeConfig, NodeRuntime};
use crate::runtime::contexts::{NodeInitContext, RenderContext};
use alloc::{boxed::Box, format, string::ToString, vec::Vec};
use lp_model::{
    NodeHandle,
    nodes::texture::{TextureConfig, TextureState},
};
use lp_shared::{Texture, fs::fs_event::FsChange};

/// Texture node runtime
pub struct TextureRuntime {
    config: Option<TextureConfig>,
    texture: Option<Texture>,
    node_handle: NodeHandle,
}

impl TextureRuntime {
    pub fn new(node_handle: NodeHandle) -> Self {
        Self {
            config: None,
            texture: None,
            node_handle,
        }
    }

    pub fn set_config(&mut self, config: TextureConfig) {
        self.config = Some(config);
    }

    pub fn texture(&self) -> Option<&Texture> {
        self.texture.as_ref()
    }

    pub fn texture_mut(&mut self) -> Option<&mut Texture> {
        self.texture.as_mut()
    }

    pub fn get_state(&self) -> TextureState {
        // Extract state for sync API
        if let Some(tex) = &self.texture {
            TextureState {
                texture_data: tex.data().to_vec(),
                width: tex.width(),
                height: tex.height(),
                format: tex.format().to_string(),
            }
        } else if let Some(config) = &self.config {
            // Fallback to config if texture not initialized
            TextureState {
                texture_data: Vec::new(),
                width: config.width,
                height: config.height,
                format: "RGBA8".to_string(), // Default format
            }
        } else {
            // No texture or config available
            TextureState {
                texture_data: Vec::new(),
                width: 0,
                height: 0,
                format: "RGBA8".to_string(),
            }
        }
    }

    /// Get the texture config (for state extraction)
    pub fn get_config(&self) -> Option<&TextureConfig> {
        self.config.as_ref()
    }
}

impl NodeRuntime for TextureRuntime {
    fn init(&mut self, _ctx: &dyn NodeInitContext) -> Result<(), Error> {
        let config = self.config.as_ref().ok_or_else(|| Error::InvalidConfig {
            node_path: format!("texture-{}", self.node_handle.as_i32()),
            reason: "Config not set".to_string(),
        })?;

        // Create texture with RGBA8 format (default for now)
        // Format will be added to TextureConfig later
        let format = "RGBA8".to_string();
        let texture = Texture::new(config.width, config.height, format).map_err(|e| {
            Error::InvalidConfig {
                node_path: format!("texture-{}", self.node_handle.as_i32()),
                reason: format!("Failed to create texture: {e}"),
            }
        })?;

        self.texture = Some(texture);
        Ok(())
    }

    fn render(&mut self, _ctx: &mut dyn RenderContext) -> Result<(), Error> {
        // No-op - textures don't render themselves, shaders render to textures
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
        _ctx: &dyn NodeInitContext,
    ) -> Result<(), Error> {
        // Downcast to TextureConfig
        let texture_config = new_config
            .as_any()
            .downcast_ref::<TextureConfig>()
            .ok_or_else(|| Error::InvalidConfig {
                node_path: format!("texture-{}", self.node_handle.as_i32()),
                reason: "Config is not a TextureConfig".to_string(),
            })?;

        let old_config = self.config.as_ref();
        let needs_resize = old_config
            .map(|old| old.width != texture_config.width || old.height != texture_config.height)
            .unwrap_or(true);

        self.config = Some(texture_config.clone());

        // If dimensions changed, resize texture
        if needs_resize {
            let format = "RGBA8".to_string();
            let texture = Texture::new(texture_config.width, texture_config.height, format)
                .map_err(|e| Error::InvalidConfig {
                    node_path: format!("texture-{}", self.node_handle.as_i32()),
                    reason: format!("Failed to resize texture: {e}"),
                })?;
            self.texture = Some(texture);
        }

        Ok(())
    }

    fn handle_fs_change(
        &mut self,
        _change: &FsChange,
        _ctx: &dyn NodeInitContext,
    ) -> Result<(), Error> {
        // Textures don't currently support loading from files
        // This is a no-op for now
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_texture_runtime_creation() {
        use lp_model::NodeHandle;
        let handle = NodeHandle::new(0);
        let runtime = TextureRuntime::new(handle);
        // Just verify it compiles and can be created
        let _boxed: alloc::boxed::Box<dyn NodeRuntime> = alloc::boxed::Box::new(runtime);
    }
}
