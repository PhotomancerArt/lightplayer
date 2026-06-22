//! UI-independent LightPlayer Studio UX surface.

pub use lpa_link::{LinkEndpointId, LinkEndpointStatus, LinkProviderKind};

pub mod node;

pub mod nodes;
pub mod ui;

pub use node::{
    ActionConfirmation, ActionEnablement, ActionMeta, ActionPriority, UiAction, UiActions,
    UxContext, UxNode, UxNodeId, UxOp,
};
pub use nodes::device::{DeviceOp, DeviceSnapshot, DeviceUx};
pub use nodes::link::{
    ConnectedDeviceSummary, ConnectedLink, EndpointChoice, LinkManagementOutcome, LinkOp,
    LinkOpenOutcome, LinkSnapshot, LinkState, LinkUx, ProgressState, ProviderChoice,
    SharedLinkRegistry, UxIssue,
};
pub use nodes::project::{
    LoadedProjectChoice, ProjectConnectResult, ProjectInventorySummary, ProjectOp, ProjectSnapshot,
    ProjectState, ProjectUx,
};
pub use nodes::server::{
    LoadedDemoProject, LoadedProjectCatalog, ServerFailureKind, ServerOp, ServerSnapshot,
    ServerState, ServerUx, StudioServerClient,
};
pub use nodes::studio::{
    StudioSnapshot, StudioUx, UxError, UxLogEntry, UxLogLevel, UxNotice, UxNoticeLevel, UxOutcome,
    UxResult, UxUpdate, UxUpdateSink,
};
pub use ui::{
    StudioView, UiActivity, UiActivityStep, UiActivityStepState, UiBody, UiMetric, UiPaneView,
    UiProgress, UiStackSection, UiStackView, UiStatus, UiStatusKind, UiStepState, UiTerminalLine,
};

pub const STUDIO_DEMO_PROJECT_ID: &str = "studio-demo";
