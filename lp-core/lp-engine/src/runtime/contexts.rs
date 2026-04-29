use crate::error::Error;
use crate::output::OutputProvider;
use lpc_model::{FrameId, NodeHandle, NodeSpecifier};
use lpfs::LpFs;

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

    /// Get current time in milliseconds (for perf timing). Returns None when no time provider.
    fn now_ms(&self) -> Option<u64> {
        None
    }

    /// Look up a texture node's configuration (dimensions, etc.).
    fn get_texture_config(
        &self,
        handle: TextureHandle,
    ) -> Result<lpl_model::nodes::texture::TextureConfig, Error>;

    /// Node handle of the shader that owns the shared CPU output buffer for this texture target.
    ///
    /// When multiple shaders write the same texture, they share one buffer (highest
    /// `render_order`, then highest node id on ties). Only that node allocates
    /// [`crate::nodes::ShaderRuntime::output_buffer`]; others write through
    /// [`RenderContext::get_target_texture_pixels_mut`].
    ///
    /// During initialization, other shaders may not be registered in the graph yet; in that case
    /// this returns `fallback_if_no_shader_yet` so the caller can allocate until peers appear.
    fn texture_output_buffer_owner(
        &self,
        handle: TextureHandle,
        fallback_if_no_shader_yet: NodeHandle,
    ) -> NodeHandle;
}

/// Context for rendering
pub trait RenderContext {
    /// Get texture (triggers lazy rendering if needed)
    fn get_texture(
        &mut self,
        handle: TextureHandle,
    ) -> Result<&dyn lps_shared::TextureBuffer, Error>;

    /// Mutable access to the shared output buffer for this texture (same allocation as
    /// [`RenderContext::get_texture`]). Used by shader `render()` so all shaders targeting a
    /// texture write the same pixels.
    fn get_target_texture_pixels_mut(
        &mut self,
        handle: TextureHandle,
    ) -> Result<&mut lp_shader::LpsTextureBuf, Error>;

    /// Get current frame time in seconds
    fn get_time(&self) -> f32;

    /// Get output buffer slice (16-bit RGB channels)
    fn get_output(
        &mut self,
        handle: OutputHandle,
        universe: u32,
        start_ch: u32,
        ch_count: u32,
    ) -> Result<&mut [u16], Error>;

    /// Get output provider
    fn output_provider(&self) -> &dyn OutputProvider;

    /// Get current frame ID
    fn frame_id(&self) -> FrameId;
}
