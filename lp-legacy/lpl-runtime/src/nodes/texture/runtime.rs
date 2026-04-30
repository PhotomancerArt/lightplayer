use alloc::{boxed::Box, format, string::ToString};
use lpc_model::{NodeId, project::FrameId};
use lpc_runtime::NodeRuntime;
use lpc_runtime::error::Error;
use lpc_runtime::output::OutputProvider;
use lpc_runtime::runtime::contexts::{NodeInitContext, RenderContext};
use lpfs::FsChange;
use lpl_model::NodeConfig;
use lpl_model::nodes::texture::{TextureConfig, TextureFormat, TextureState};

/// Texture node runtime
pub struct TextureRuntime {
    config: Option<TextureConfig>,
    pub state: TextureState,
    node_handle: NodeId,
}

impl TextureRuntime {
    pub fn new(node_handle: NodeId) -> Self {
        Self {
            config: None,
            state: TextureState::new(FrameId::default()),
            node_handle,
        }
    }

    pub fn set_config(&mut self, config: TextureConfig) {
        self.config = Some(config);
    }

    pub fn get_state(&self) -> TextureState {
        self.state.clone()
    }

    /// Get the texture config (for state extraction)
    pub fn get_config(&self) -> Option<&TextureConfig> {
        self.config.as_ref()
    }

    fn sync_state_from_config(&mut self) {
        let Some(config) = self.config.as_ref() else {
            return;
        };
        let frame_id = FrameId::default();
        let format = TextureFormat::Rgba16;
        // TODO(M4a): texture_data state should come from upstream shader's buffer
        self.state
            .texture_data
            .set(frame_id, alloc::vec::Vec::new());
        self.state.width.set(frame_id, config.width);
        self.state.height.set(frame_id, config.height);
        self.state.format.set(frame_id, format);
    }
}

impl NodeRuntime for TextureRuntime {
    fn init(&mut self, _ctx: &dyn NodeInitContext) -> Result<(), Error> {
        self.config.as_ref().ok_or_else(|| Error::InvalidConfig {
            node_path: format!("texture-{}", self.node_handle.as_u32()),
            reason: "Config not set".to_string(),
        })?;
        self.sync_state_from_config();
        Ok(())
    }

    fn render(&mut self, _ctx: &mut dyn RenderContext) -> Result<(), Error> {
        Ok(())
    }

    fn shed_optional_buffers(
        &mut self,
        _output_provider: Option<&dyn OutputProvider>,
    ) -> Result<(), Error> {
        self.state
            .texture_data
            .set(FrameId::default(), alloc::vec::Vec::new());
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
        let texture_config = new_config
            .as_any()
            .downcast_ref::<TextureConfig>()
            .ok_or_else(|| Error::InvalidConfig {
                node_path: format!("texture-{}", self.node_handle.as_u32()),
                reason: "Config is not a TextureConfig".to_string(),
            })?;

        self.config = Some(texture_config.clone());
        self.sync_state_from_config();

        Ok(())
    }

    fn handle_fs_change(
        &mut self,
        _change: &FsChange,
        _ctx: &dyn NodeInitContext,
    ) -> Result<(), Error> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_texture_runtime_creation() {
        use lpc_model::NodeId;
        let handle = NodeId::new(0);
        let runtime = TextureRuntime::new(handle);
        let _boxed: alloc::boxed::Box<dyn NodeRuntime> = alloc::boxed::Box::new(runtime);
    }
}
