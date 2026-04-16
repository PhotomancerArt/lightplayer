//! Effect engine and per-instance runtime traits.

use crate::input::FxValue;
use crate::module::FxModule;
use crate::texture::{TextureFormat, TextureId};

/// Compiles effects and allocates [`CpuTexture`](crate::texture::CpuTexture) storage.
pub trait FxEngine {
    type Instance: FxInstance;
    type Error: core::fmt::Display;

    fn create_texture(&mut self, width: u32, height: u32, format: TextureFormat) -> TextureId;

    fn instantiate(
        &mut self,
        module: &FxModule,
        output: TextureId,
    ) -> Result<Self::Instance, Self::Error>;
}

/// One runnable effect: uniforms from manifest inputs + periodic [`FxInstance::render`].
pub trait FxInstance {
    type Error: core::fmt::Display;

    fn set_input(&mut self, name: &str, value: FxValue) -> Result<(), Self::Error>;

    fn render(&mut self, time: f32) -> Result<(), Self::Error>;
}
