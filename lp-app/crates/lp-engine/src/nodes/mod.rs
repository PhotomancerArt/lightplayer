use crate::error::Error;
use crate::runtime::contexts::{NodeInitContext, RenderContext};
use alloc::boxed::Box;
use lp_shared::fs::fs_event::FsChange;

pub mod fixture;
pub mod output;
pub mod shader;
pub mod texture;

pub use fixture::FixtureRuntime;
pub use output::OutputRuntime;
pub use shader::ShaderRuntime;
pub use texture::TextureRuntime;

use core::any::Any;

/// Node runtime trait - all node runtimes implement this
pub trait NodeRuntime: Send + Sync {
    /// Initialize the node
    fn init(&mut self, ctx: &dyn NodeInitContext) -> Result<(), Error>;

    /// Render the node
    fn render(&mut self, ctx: &mut dyn RenderContext) -> Result<(), Error>;

    /// Destroy the node (cleanup)
    ///
    /// Called when a node is being removed or the runtime is shutting down.
    /// Default implementation does nothing - nodes can override if they need cleanup.
    fn destroy(&mut self) -> Result<(), Error> {
        Ok(())
    }

    /// Get reference to Any for downcasting
    fn as_any(&self) -> &dyn Any;

    /// Get mutable reference to Any for downcasting
    fn as_any_mut(&mut self) -> &mut dyn Any;

    /// Update the node's configuration
    ///
    /// Called when node.json changes. Nodes can choose to reinit or update in place.
    fn update_config(
        &mut self,
        new_config: Box<dyn NodeConfig>,
        ctx: &dyn NodeInitContext,
    ) -> Result<(), Error>;

    /// Handle filesystem changes to non-config files
    ///
    /// Called when files other than node.json change (e.g., main.glsl for shaders).
    fn handle_fs_change(
        &mut self,
        change: &FsChange,
        ctx: &dyn NodeInitContext,
    ) -> Result<(), Error>;
}

// Re-export NodeConfig from lp-model
pub use lp_model::NodeConfig;
