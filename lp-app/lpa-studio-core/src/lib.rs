//! Headless LightPlayer Studio application core.

pub use lpa_link::{LinkEndpointId, LinkEndpointStatus, LinkProviderKind};

pub mod app;
pub mod controller;
pub mod core;

pub use self::core::status::UiStatusKind;
pub use app::device::{DeviceController, DeviceOp, DeviceSnapshot};
pub use app::link::{
    ConnectedDeviceSummary, ConnectedLink, EndpointChoice, LinkController, LinkManagementOutcome,
    LinkOp, LinkOpenOutcome, LinkSnapshot, LinkState, ProgressState, ProviderChoice,
    SharedLinkRegistry, UiIssue,
};
pub use app::node::{
    UiAssetEditorKind, UiBindingEndpoint, UiConfigSlot, UiConfigSlotBody, UiNodeChild,
    UiNodeDirtyState, UiNodeHeader, UiNodeSection, UiNodeTab, UiNodeTabBody, UiNodeView,
    UiProducedBinding, UiProducedBindings, UiProducedProduct, UiProducedValue, UiProductKind,
    UiSlotAffordance, UiSlotAspect, UiSlotAspectKind, UiSlotAspectRow, UiSlotAsset,
    UiSlotEditorHint, UiSlotFieldState, UiSlotOption, UiSlotRecord, UiSlotShape, UiSlotShapeField,
    UiSlotSourceState, UiSlotUnit, UiSlotValue, UiSlotValueKind,
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
    StudioController, StudioSnapshot, UiError, UiLogEntry, UiLogLevel, UiNotice, UiNoticeLevel,
    UiResult, UxActivityTarget, UxUpdate, UxUpdateSink,
};
pub use core::notice::UiNotices;
pub use core::view::activity_view::UiActivityStep;
pub use core::view::activity_view::UiActivityStepState;
pub use core::view::steps_view::UiStepState;
pub use core::view::steps_view::UiStepView;
pub use core::{
    ActionConfirmation, ActionEnablement, ActionMeta, ActionPriority, Controller,
    ControllerContext, ControllerId, ControllerOp, UiAction, UiActions, UiActivityView, UiMetric,
    UiPaneView, UiProgress, UiStatus, UiStepsView, UiStudioView, UiTerminalLine, UiViewContent,
    UxNodePath,
};

pub const STUDIO_DEMO_PROJECT_ID: &str = "studio-demo";
