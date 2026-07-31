//! Studio-specific UI surfaces.
//!
//! These components know about LightPlayer Studio concepts such as devices,
//! projects, nodes, and the overall Studio shell. They compose `core`
//! controls and `base` primitives into app-specific workflows.

pub(crate) mod affordance;
pub mod bus;
pub mod device;
pub mod home;
pub mod layout;
#[cfg(feature = "stories")]
pub(crate) mod mapping_editor_stories;
/// M2 UX spike: the module face, panel, and play-mode surfaces.
pub mod module;
pub mod node;
pub mod project;
#[cfg(feature = "stories")]
pub(crate) mod readme_stories;
pub mod roster;
#[cfg(feature = "stories")]
pub(crate) mod story_fixtures;

pub use bus::BusPaneBody;
pub use home::{HomeGallery, ProjectOpeningFrame};
pub use layout::{PaneFrame, StudioShell};
pub use node::NodePane;
pub use project::{ProjectNodeWorkspace, ProjectPane};
