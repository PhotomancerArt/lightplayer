//! Studio-specific UI surfaces.
//!
//! These components know about LightPlayer Studio concepts such as devices,
//! projects, nodes, and the overall Studio shell. They compose `core`
//! controls and `base` primitives into app-specific workflows.

pub(crate) mod affordance;
pub mod device;
pub mod home;
pub mod layout;
pub mod node;
pub mod project;
#[cfg(feature = "stories")]
pub(crate) mod story_fixtures;

pub use device::RuntimeLog;
pub use home::HomeGallery;
pub use layout::{PaneFrame, StudioShell};
pub use node::NodePane;
pub use project::{ProjectNodeWorkspace, ProjectPane};
