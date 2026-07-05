pub mod pane_frame;
pub mod studio_shell;
#[cfg(feature = "stories")]
pub(crate) mod studio_shell_stories;
pub mod version_badge;
#[cfg(feature = "stories")]
pub(crate) mod version_badge_stories;

pub use pane_frame::PaneFrame;
pub use studio_shell::StudioShell;
pub use version_badge::VersionBadge;
