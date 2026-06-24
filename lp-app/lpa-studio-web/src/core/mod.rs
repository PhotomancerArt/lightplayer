//! Data-driven UI controls.
//!
//! These components render generic `Ui*` structs from `lpa-studio-core`.
//! They may use `base` primitives, but should avoid owning Studio domain
//! workflows directly when an `app` component can compose them instead.

pub mod action;
pub mod app_activity;
pub mod app_body;
pub mod app_progress;
pub mod metric_grid;
pub mod status_chip;

pub use crate::view::app_pane::AppPane;
pub use crate::view::app_stack::AppStack;
pub use action::{ActionButton, ActionStrip};
pub use app_activity::AppActivity;
pub use app_body::AppBody;
pub use app_progress::AppProgress;
pub use metric_grid::MetricGrid;
pub use status_chip::StatusChip;
