//! Data-driven UI contracts shared by Studio controllers and renderers.
//!
//! Types in this module describe what the UI should render, not how a specific
//! frontend should render it. Controllers produce `Ui*` data, while web
//! components in `lpa-studio-web/src/core` consume the same data.

pub mod action;
pub mod error;
pub mod issue;
pub mod log;
pub mod metric;
pub mod notice;
pub mod progress;
pub mod status;
pub mod terminal_line;
pub mod view;

pub use crate::app::studio::ui_studio_view::UiStudioView;
pub use crate::controller::{
    ActionClass, ActionConfirmation, ActionEnablement, ActionMeta, ActionPriority, Controller,
    ControllerContext, ControllerId, ControllerOp, PASSIVE_REFRESH_DEADLINE,
    PROJECT_ACTION_DEADLINE, PROJECT_EDITOR_ACTION_DEADLINE, PROJECT_LOAD_DEADLINE, UiAction,
    UiActions, UxNodePath,
};
pub use metric::UiMetric;
pub use progress::UiProgress;
pub use status::UiStatus;
pub use status::UiStatusKind;
pub use terminal_line::UiTerminalLine;
pub use view::activity_view::UiActivityStep;
pub use view::activity_view::UiActivityStepState;
pub use view::activity_view::UiActivityView;
pub use view::pane_view::UiPaneView;
pub use view::steps_view::UiStepState;
pub use view::steps_view::UiStepView;
pub use view::steps_view::UiStepsView;
pub use view::view_content::UiViewContent;
