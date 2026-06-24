//! Data-driven UI controls.
//!
//! These components render generic `Ui*` structs from `lpa-studio-core`.
//! They may use `base` primitives, but should avoid owning Studio domain
//! workflows directly when an `app` component can compose them instead.

pub mod action;
pub mod metric_grid;
pub mod progress_bar;
pub mod status_chip;
pub mod view;

pub use action::{ActionButton, ActionStrip};
pub use metric_grid::MetricGrid;
pub use progress_bar::ProgressBar;
pub use status_chip::StatusChip;
pub use view::activity_view::ActivityView;
pub use view::pane_view::PaneView;
pub use view::stack_view::StepsView;
pub use view::view_content::ViewContent;
