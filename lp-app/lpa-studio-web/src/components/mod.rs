//! Dioxus components for the active Studio UX shell.

mod action_button;
mod action_strip;
mod field_row;
mod metric_grid;
mod pane_frame;
mod project_workspace;
mod runtime_log;
mod status_chip;
mod studio_icon;
mod studio_shell;
mod tabs;
mod ux_pane;

pub use action_button::ActionButton;
pub use action_strip::ActionStrip;
pub use field_row::FieldRow;
pub use metric_grid::MetricGrid;
pub use pane_frame::PaneFrame;
pub use project_workspace::{ProjectNodeWorkspace, ProjectSidebar};
pub use runtime_log::RuntimeLog;
pub use status_chip::StatusChip;
pub use studio_icon::{StudioIcon, StudioIconName, action_icon_name};
pub use studio_shell::StudioShell;
pub use tabs::{TabItem, Tabs};
pub use ux_pane::UxPane;
