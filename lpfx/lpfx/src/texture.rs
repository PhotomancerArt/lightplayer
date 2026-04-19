//! Opaque texture handles for effect outputs.

/// Opaque texture handle issued by [`crate::FxEngine::create_texture`](crate::engine::FxEngine::create_texture).
///
/// CPU backends use this as a key into their internal texture pool
/// (typically a `BTreeMap<TextureId, …>`). The actual pixel buffer
/// type is backend-specific and not surfaced through this trait.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TextureId(u32);

impl TextureId {
    /// Wrap a raw id. Backends allocate unique ids when creating textures.
    #[must_use]
    pub const fn from_raw(id: u32) -> Self {
        Self(id)
    }

    /// Raw id for maps and logging.
    #[must_use]
    pub const fn raw(self) -> u32 {
        self.0
    }
}
