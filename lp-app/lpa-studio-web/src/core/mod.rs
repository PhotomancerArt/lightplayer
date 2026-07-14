//! Data-driven UI controls.
//!
//! These components render generic `Ui*` structs from `lpa-studio-core`.
//! They may use `base` primitives, but should avoid owning Studio domain
//! workflows directly when an `app` component can compose them instead.

pub mod action;
pub mod issue_view;
#[cfg(feature = "stories")]
pub(crate) mod issue_view_stories;
pub mod log_list;
#[cfg(feature = "stories")]
pub(crate) mod log_list_stories;
pub mod metric_grid;
#[cfg(feature = "stories")]
pub(crate) mod metric_grid_stories;
pub mod progress_bar;
#[cfg(feature = "stories")]
pub(crate) mod progress_bar_stories;
pub mod status_chip;
#[cfg(feature = "stories")]
pub(crate) mod status_chip_stories;
#[cfg(feature = "stories")]
pub(crate) mod story_fixtures;
pub mod terminal_output;
#[cfg(feature = "stories")]
pub(crate) mod terminal_output_stories;
pub mod view;

pub use action::{
    ActionButton, ActionButtonVariant, ActionStrip, menu_item_action_class, quiet_action_class,
};
pub use issue_view::IssueView;
pub use log_list::LogList;
pub use metric_grid::MetricGrid;
pub use progress_bar::ProgressBar;
pub use status_chip::StatusChip;
pub use terminal_output::TerminalOutput;
pub use view::activity_view::ActivityView;
pub use view::pane_view::PaneView;
pub use view::stack_view::StepsView;
pub use view::view_content::ViewContent;
