#[cfg(feature = "stories")]
pub(crate) mod editor_fields_stories;
pub mod project_workspace;
#[cfg(feature = "stories")]
pub(crate) mod project_workspace_stories;

pub use project_workspace::{ProjectNodeWorkspace, ProjectSidebar, ProjectWorkspace};
