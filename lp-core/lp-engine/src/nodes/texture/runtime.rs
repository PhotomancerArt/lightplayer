use crate::error::Error;
use crate::nodes::{NodeConfig, NodeRuntime};
use crate::runtime::contexts::{NodeInitContext, RenderContext};
use alloc::{boxed::Box, format, string::ToString, vec::Vec};
use lp_model::{
    NodeHandle,
    nodes::texture::{TextureConfig, TextureState},
    project::FrameId,
};
use lp_shared::{Texture, fs::fs_event::FsChange};

/// Texture node runtime
pub struct TextureRuntime {
    config: Option<TextureConfig>,
    texture: Option<Texture>,
    pub state: TextureState,
    node_handle: NodeHandle,
}

impl TextureRuntime {
    pub fn new(node_handle: NodeHandle) -> Self {
        Self {
            config: None,
            texture: None,
            state: TextureState::new(FrameId::default()),
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
        // Return cloned state
        self.state.clone()
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

        // Update state with texture data
        if let Some(tex) = &self.texture {
            let frame_id = FrameId::default(); // NodeInitContext doesn't provide frame_id
            self.state.texture_data.set(frame_id, tex.data().to_vec());
            self.state.width.set(frame_id, tex.width());
            self.state.height.set(frame_id, tex.height());
            self.state.format.set(frame_id, tex.format().to_string());
        }

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

            // Update state with new texture data
            if let Some(tex) = &self.texture {
                let frame_id = FrameId::default(); // NodeInitContext doesn't provide frame_id
                self.state.texture_data.set(frame_id, tex.data().to_vec());
                self.state.width.set(frame_id, tex.width());
                self.state.height.set(frame_id, tex.height());
                self.state.format.set(frame_id, tex.format().to_string());
            }
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
