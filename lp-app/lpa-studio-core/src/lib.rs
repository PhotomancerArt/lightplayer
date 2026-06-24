//! Headless LightPlayer Studio application core.

pub use lpa_link::{LinkEndpointId, LinkEndpointStatus, LinkProviderKind};

pub mod app;
pub mod core;
pub mod node;
pub mod view;

pub use self::core::activity::UiActivityStep;
pub use self::core::activity::UiActivityStepState;
pub use self::core::status::UiStatusKind;
pub use app::device::{DeviceController, DeviceOp, DeviceSnapshot};
pub use app::link::{
    ConnectedDeviceSummary, ConnectedLink, EndpointChoice, LinkController, LinkManagementOutcome,
    LinkOp, LinkOpenOutcome, LinkSnapshot, LinkState, ProgressState, ProviderChoice,
    SharedLinkRegistry, UxIssue,
};
pub use app::project::{
    LoadedProjectChoice, ProjectConnectResult, ProjectController, ProjectEditorOp,
    ProjectEditorTarget, ProjectEditorView, ProjectInventorySummary, ProjectNodeStatusTone,
    ProjectNodeStatusView, ProjectNodeTreeItem, ProjectNodeTreeView, ProjectNodeView, ProjectOp,
    ProjectRuntimeSummary, ProjectSlotGroupView, ProjectSlotIssueView, ProjectSlotRowView,
    ProjectSlotValueView, ProjectSnapshot, ProjectState, ProjectSync, ProjectSyncPhase,
    ProjectSyncRun, ProjectSyncSummary,
};
pub use app::server::{
    LoadedDemoProject, LoadedProjectCatalog, ServerController, ServerFailureKind, ServerOp,
    ServerSnapshot, ServerState, StudioProjectRead, StudioServerClient,
};
pub use app::studio::{
    NoticeLevel, StudioController, StudioSnapshot, UiError, UiLogEntry, UiLogLevel, UiNotice,
    UxActivityTarget, UxResult, UxUpdate, UxUpdateSink,
};
pub use core::notice::UiNotices;
pub use core::{
    ActionConfirmation, ActionEnablement, ActionMeta, ActionPriority, UiAction, UiActions,
    UiActivity, UiMetric, UiPaneView, UiProgress, UiStatus, UiStepsView, UiStudioView,
    UiTerminalLine, UiViewContent, UxContext, UxNode, UxNodeId, UxNodePath, UxOp,
};
pub use view::steps_view::UiStepState;
pub use view::steps_view::UiStepView;

pub const STUDIO_DEMO_PROJECT_ID: &str = "studio-demo";
