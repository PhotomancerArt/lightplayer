//! Descriptor for pixel shader compilation (GLSL + output format + texture binding specs).

use alloc::collections::BTreeMap;
use alloc::string::String;

use lpir::CompilerConfig;
use lps_shared::{TextureBindingSpec, TextureStorageFormat};

pub type TextureBindingSpecs = BTreeMap<String, TextureBindingSpec>;

pub struct CompilePxDesc<'a> {
    pub glsl: &'a str,
    pub output_format: TextureStorageFormat,
    pub compiler_config: CompilerConfig,
    pub textures: TextureBindingSpecs,
}

impl<'a> CompilePxDesc<'a> {
    /// Build a descriptor with no texture binding specs.
    #[must_use]
    pub fn new(
        glsl: &'a str,
        output_format: TextureStorageFormat,
        compiler_config: CompilerConfig,
    ) -> Self {
        Self {
            glsl,
            output_format,
            compiler_config,
            textures: TextureBindingSpecs::new(),
        }
    }
}
