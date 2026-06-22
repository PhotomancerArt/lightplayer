//! UI-independent LightPlayer Studio UX surface.

pub mod action;

#[cfg(all(feature = "browser-worker", target_arch = "wasm32"))]
mod browser_worker;

pub mod link;
pub mod project;
pub mod server;
pub mod studio;

pub use action::{
    ActionConfirmation, ActionEnablement, ActionKind, ActionMeta, ActionPriority, AvailableAction,
    UxCommand,
};
pub use link::{
    ConnectedDeviceSummary, LinkAction, LinkSnapshot, LinkState, LinkUx, ProgressState,
    ProviderChoice, UxIssue,
};
pub use project::{
    ProjectAction, ProjectInventorySummary, ProjectSnapshot, ProjectState, ProjectUx,
};
pub use server::{ServerSnapshot, ServerState, ServerUx};
pub use studio::{
    StudioAction, StudioSnapshot, StudioUx, UxError, UxLogEntry, UxLogLevel, UxNotice,
    UxNoticeLevel, UxOutcome, UxResult,
};

pub const STUDIO_DEMO_PROJECT_ID: &str = "studio-demo";
