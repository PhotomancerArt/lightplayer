//! The project-scoped patch surface (D36, slice 2).

mod patch_surface;
#[cfg(feature = "stories")]
pub(crate) mod patch_surface_stories;

pub use patch_surface::PatchSurfacePage;
