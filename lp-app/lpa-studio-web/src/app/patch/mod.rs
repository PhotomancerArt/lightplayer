//! The project-scoped patch surface (D36, slice 2) and the verb-UI
//! helpers the workbench Patching view dispatches through.

mod patch_surface;
#[cfg(feature = "stories")]
pub(crate) mod patch_surface_stories;
pub(crate) mod verb_ui;

pub use patch_surface::PatchSurfacePage;
