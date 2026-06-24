//! Data-driven UI controls.
//!
//! These components render generic `Ui*` structs from `lpa-studio-core`.
//! They may use `base` primitives, but should avoid owning Studio domain
//! workflows directly when an `app` component can compose them instead.

pub mod actions;
pub mod app_activity;
pub mod app_body;
pub mod app_pane;
pub mod app_progress;
pub mod app_stack;
pub mod metric_grid;
pub mod status_chip;

pub use actions::{ActionButton, ActionStrip};
pub use app_activity::AppActivity;
pub use app_body::AppBody;
pub use app_pane::AppPane;
pub use app_progress::AppProgress;
pub use app_stack::AppStack;
pub use metric_grid::MetricGrid;
pub use status_chip::StatusChip;
