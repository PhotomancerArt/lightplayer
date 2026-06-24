//! Headless LightPlayer Studio application core.

pub use lpa_link::{LinkEndpointId, LinkEndpointStatus, LinkProviderKind};

pub mod app;
pub mod core;

pub use app::device::{DeviceOp, DeviceSnapshot, DeviceUx};
pub use app::link::{
    ConnectedDeviceSummary, ConnectedLink, EndpointChoice, LinkManagementOutcome, LinkOp,
    LinkOpenOutcome, LinkSnapshot, LinkState, LinkUx, ProgressState, ProviderChoice,
    SharedLinkRegistry, UxIssue,
};
pub use app::project::{
    LoadedProjectChoice, ProjectConnectResult, ProjectEditorOp, ProjectEditorTarget,
    ProjectEditorView, ProjectInventorySummary, ProjectNodeStatusTone, ProjectNodeStatusView,
    ProjectNodeTreeItem, ProjectNodeTreeView, ProjectNodeView, ProjectOp, ProjectRuntimeSummary,
    ProjectSlotGroupView, ProjectSlotIssueView, ProjectSlotRowView, ProjectSlotValueView,
    ProjectSnapshot, ProjectState, ProjectSync, ProjectSyncPhase, ProjectSyncRun,
    ProjectSyncSummary, ProjectUx,
};
pub use app::server::{
    LoadedDemoProject, LoadedProjectCatalog, ServerFailureKind, ServerOp, ServerSnapshot,
    ServerState, ServerUx, StudioProjectRead, StudioServerClient,
};
pub use app::studio::{
    StudioSnapshot, StudioUx, UxActivityTarget, UxError, UxLogEntry, UxLogLevel, UxNotice,
    UxNoticeLevel, UxOutcome, UxResult, UxUpdate, UxUpdateSink,
};
pub use core::{
    ActionConfirmation, ActionEnablement, ActionMeta, ActionPriority, StudioView, UiAction,
    UiActions, UiActivity, UiActivityStep, UiActivityStepState, UiBody, UiMetric, UiPaneView,
    UiProgress, UiStackSection, UiStackView, UiStatus, UiStatusKind, UiStepState, UiTerminalLine,
    UxContext, UxNode, UxNodeId, UxNodePath, UxOp,
};

pub const STUDIO_DEMO_PROJECT_ID: &str = "studio-demo";
