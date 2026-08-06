//! Studio-specific UI surfaces.
//!
//! These components know about LightPlayer Studio concepts such as devices,
//! projects, nodes, and the overall Studio shell. They compose `core`
//! controls and `base` primitives into app-specific workflows.

pub(crate) mod affordance;
#[cfg(feature = "stories")]
pub(crate) mod board_diagram_stories;
#[cfg(feature = "stories")]
pub(crate) mod board_editor_stories;
pub mod docs;
pub mod home;
pub mod layout;
#[cfg(feature = "stories")]
pub(crate) mod mapping_editor_stories;
/// The module face, panel, and play-mode surfaces.
pub mod module;
pub mod node;
pub mod project;
#[cfg(feature = "stories")]
pub(crate) mod readme_stories;
pub mod roster;
pub mod section_stubs;
#[cfg(feature = "stories")]
pub(crate) mod story_fixtures;
pub mod wiring;

pub use docs::DocsPage;
pub use home::{HomeGallery, ProjectOpeningFrame};
pub use section_stubs::{ExplorePage, HomePage};
pub use layout::{PaneFrame, StudioShell};
pub use node::NodePane;
pub use project::{ProjectNodeWorkspace, ProjectPane};
pub use wiring::WiringDrawerBody;
