//! UI-independent LightPlayer Studio UX surface.

pub use lpa_link::{LinkEndpointId, LinkEndpointStatus, LinkProviderKind};

pub mod action;

pub mod link;
pub mod project;
pub mod server;
pub mod studio;
pub mod view;

pub use action::{
    ActionConfirmation, ActionEnablement, ActionMeta, ActionPriority, UxAction, UxActions,
    UxContext, UxNode, UxNodeId, UxOp,
};
pub use link::{
    ConnectedDeviceSummary, ConnectedLink, EndpointChoice, LinkOp, LinkOpenOutcome, LinkSnapshot,
    LinkState, LinkUx, ProgressState, ProviderChoice, SharedLinkRegistry, UxIssue,
};
pub use project::{
    LoadedProjectChoice, ProjectConnectResult, ProjectInventorySummary, ProjectOp, ProjectSnapshot,
    ProjectState, ProjectUx,
};
pub use server::{
    LoadedDemoProject, LoadedProjectCatalog, ServerOp, ServerSnapshot, ServerState, ServerUx,
    StudioServerClient,
};
pub use studio::{
    StudioSnapshot, StudioUx, UxError, UxLogEntry, UxLogLevel, UxNotice, UxNoticeLevel, UxOutcome,
    UxResult,
};
pub use view::{StudioView, UxBody, UxMetric, UxPaneView, UxStatus, UxStatusKind};

pub const STUDIO_DEMO_PROJECT_ID: &str = "studio-demo";
