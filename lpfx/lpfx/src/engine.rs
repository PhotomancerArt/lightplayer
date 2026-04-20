//! Effect engine and per-instance runtime traits.

use crate::module::FxModule;
use crate::render_inputs::FxRenderInputs;
use crate::texture::TextureId;

/// Compiles effects and allocates output texture storage.
///
/// CPU backends only support `Rgba16Unorm` today; if a second format
/// is needed, surface `lps_shared::TextureStorageFormat` directly
/// rather than reintroducing a parallel `lpfx`-side enum.
pub trait FxEngine {
    type Instance: FxInstance;
    type Error: core::fmt::Display;

    /// Allocate a texture (`Rgba16Unorm`) and return an opaque handle.
    fn create_texture(&mut self, width: u32, height: u32) -> TextureId;

    fn instantiate(
        &mut self,
        module: &FxModule,
        output: TextureId,
    ) -> Result<Self::Instance, Self::Error>;
}

/// One runnable effect: render one frame, supplying all uniforms per call.
pub trait FxInstance {
    type Error: core::fmt::Display;

    /// Render one frame using the supplied inputs.
    ///
    /// `inputs.time` is the frame clock; `inputs.inputs` is a slice
    /// of `(name, value)` pairs matching manifest input names. The
    /// implementation maps each `name` to its `input_<name>`
    /// uniform.
    fn render(&mut self, inputs: &FxRenderInputs<'_>) -> Result<(), Self::Error>;
}
