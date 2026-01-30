use crate::error::Error;
use crate::output::OutputProvider;
use lp_model::{NodeHandle, NodeSpecifier};
use lp_shared::fs::LpFs;

/// Handle for resolved texture nodes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextureHandle(NodeHandle);

impl TextureHandle {
    pub fn new(handle: NodeHandle) -> Self {
        Self(handle)
    }

    pub fn as_node_handle(&self) -> NodeHandle {
        self.0
    }
}

/// Handle for resolved output nodes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OutputHandle(NodeHandle);

impl OutputHandle {
    pub fn new(handle: NodeHandle) -> Self {
        Self(handle)
    }

    pub fn as_node_handle(&self) -> NodeHandle {
        self.0
    }
}

/// Context for node initialization
pub trait NodeInitContext {
    /// Resolve a node specifier to a node handle (common method)
    fn resolve_node(&self, spec: &NodeSpecifier) -> Result<NodeHandle, Error>;

    /// Resolve an output node specifier to a handle
    fn resolve_output(&self, spec: &NodeSpecifier) -> Result<OutputHandle, Error>;

    /// Resolve a texture node specifier to a handle
    fn resolve_texture(&self, spec: &NodeSpecifier) -> Result<TextureHandle, Error>;

    /// Get filesystem for this node
    fn get_node_fs(&self) -> &dyn LpFs;

    /// Get output provider
    fn output_provider(&self) -> &dyn OutputProvider;
}

use lp_shared::Texture;

/// Context for rendering
pub trait RenderContext {
    /// Get texture (triggers lazy rendering if needed)
    fn get_texture(&mut self, handle: TextureHandle) -> Result<&Texture, Error>;

    /// Get mutable texture (triggers lazy rendering if needed)
    fn get_texture_mut(&mut self, handle: TextureHandle) -> Result<&mut Texture, Error>;

    /// Get current frame time in seconds
    fn get_time(&self) -> f32;

    /// Get output buffer slice
    fn get_output(
        &mut self,
        handle: OutputHandle,
        universe: u32,
        start_ch: u32,
        ch_count: u32,
    ) -> Result<&mut [u8], Error>;

    /// Get output provider
    fn output_provider(&self) -> &dyn OutputProvider;
}
