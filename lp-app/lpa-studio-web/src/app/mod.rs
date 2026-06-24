//! Studio-specific UI surfaces.
//!
//! These components know about LightPlayer Studio concepts such as devices,
//! projects, nodes, and the overall Studio shell. They compose `core`
//! controls and `base` primitives into app-specific workflows.

pub mod device;
pub mod layout;
pub mod node;
pub mod project;
#[cfg(feature = "stories")]
pub(crate) mod story_fixtures;

pub use device::RuntimeLog;
pub use layout::{PaneFrame, StudioShell};
pub use project::{ProjectNodeWorkspace, ProjectSidebar, ProjectWorkspace};
