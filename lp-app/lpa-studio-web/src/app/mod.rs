//! Studio-specific UI surfaces.
//!
//! These components know about LightPlayer Studio concepts such as devices,
//! projects, nodes, and the overall Studio shell. They compose `core`
//! controls and `base` primitives into app-specific workflows.

pub mod account;
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
pub mod patch;
pub mod project;
#[cfg(feature = "stories")]
pub(crate) mod readme_stories;
pub mod roster;
/// The owner's sharing surface: the Share pill, its panel, and the archive.
pub mod share;
#[cfg(feature = "stories")]
pub(crate) mod story_fixtures;
pub mod wiring;
pub mod workbench;

pub use account::AccountPage;
pub use docs::DocsPage;
pub use home::{DevicesPage, ExplorePage, HomePage, ProjectOpeningFrame, ProjectsPage};
pub use layout::{PaneFrame, StudioShell};
pub use node::NodePane;
pub use project::{ProjectNodeWorkspace, ProjectPane};
pub use share::{ArchivedProjectsSection, ProjectShareControl};
pub use wiring::WiringDrawerBody;
