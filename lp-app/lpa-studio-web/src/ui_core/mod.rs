//! Data-driven UI controls.
//!
//! These components render generic `Ui*` structs from `lpa-studio-ux`.
//! They may use `ui_base` primitives, but should avoid owning Studio domain
//! workflows directly when a `ui_studio` component can compose them instead.

pub mod action_button;
pub mod action_strip;
pub mod app_activity;
pub mod app_body;
pub mod app_pane;
pub mod app_progress;
pub mod app_stack;
pub mod metric_grid;
pub mod status_chip;

pub use action_button::ActionButton;
pub use action_strip::ActionStrip;
pub use app_activity::AppActivity;
pub use app_body::AppBody;
pub use app_pane::AppPane;
pub use app_progress::AppProgress;
pub use app_stack::AppStack;
pub use metric_grid::MetricGrid;
pub use status_chip::StatusChip;
