//! Headless LightPlayer Studio application core.

pub use lpa_link::{LinkEndpointId, LinkEndpointStatus, LinkProviderKind};
pub use lpc_model::{
    ArtifactLocation, ColorOrder, ControlDisplayLayout, ControlExtent, ControlLamp2d,
    ControlLayout2d, ControlSampleEncoding, ControlSampleLayout, ControlSampleSpan, LpValue,
    Revision, SlotMapKey, SlotPath, SlotPathSegment,
};

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
    UiAssetEditorKind, UiAssetEditorTab, UiBindingEndpoint, UiConfigSlot, UiConfigSlotBody,
    UiControlProductPreview, UiControlSampleFormat, UiNodeChild, UiNodeDirtyState, UiNodeHeader,
    UiNodeSection, UiNodeTab, UiNodeTabBody, UiNodeView, UiProducedBinding, UiProducedBindings,
    UiProducedProduct, UiProducedValue, UiProductKind, UiProductPreview, UiProductPreviewFrame,
    UiProductRef, UiProductTrackingState, UiSlotAffordance, UiSlotAspect, UiSlotAspectKind,
    UiSlotAspectRow, UiSlotAsset, UiSlotComposite, UiSlotEditorHint, UiSlotEnumComposite,
    UiSlotFieldState, UiSlotMapComposite, UiSlotMapKeyKind, UiSlotOption, UiSlotOptionality,
    UiSlotRecord, UiSlotShape, UiSlotShapeField, UiSlotSourceState, UiSlotUnit, UiSlotValue,
    UiSlotValueKind,
};
pub use app::project::{
    AssetContentFetchOp, AssetEditOp, DirtySummary, LoadedProjectChoice, MAX_ASSET_BODY_BYTES,
    NodeController, NodeControllerState, NodeRevertOp, PendingAssetEdit, PendingEdit,
    PendingEditOp, PendingEditPhase, ProjectAssetContentRun, ProjectConnectResult,
    ProjectController, ProjectEditRun, ProjectEditorOp, ProjectEditorTarget, ProjectEditorView,
    ProjectInventorySummary, ProjectNodeAddress, ProjectNodeStatusTone, ProjectNodeStatusView,
    ProjectNodeTarget, ProjectNodeTreeItem, ProjectNodeTreeView, ProjectOp,
    ProjectProductSubscriptionIntent, ProjectRefreshOutcome, ProjectRuntimeSummary,
    ProjectSlotAddress, ProjectSlotRoot, ProjectSnapshot, ProjectState, ProjectSync,
    ProjectSyncPhase, ProjectSyncRun, ProjectSyncSummary, SlotController, SlotControllerState,
    SlotEditOp, SlotKind, UiAffordance, UiAssetContent, UiAssetContentBody, UiPendingEdit,
    UiPendingEditKind, UiPendingEditPhase, UiShaderError,
};
pub use app::server::{
    LoadedDemoProject, LoadedProjectCatalog, ServerController, ServerFailureKind, ServerOp,
    ServerSnapshot, ServerState, StudioFsRead, StudioOverlayCommit, StudioOverlayMutation,
    StudioOverlayRead, StudioProjectRead, StudioProjectReadOutcome, StudioServerClient,
};
pub use app::studio::{
    ConsoleCommand, DEVICE_REFRESH_INTERVAL, LOG_RING_CAPACITY, LogClock, LogFilter, LogRing,
    RefreshCadence, SIMULATOR_REFRESH_INTERVAL, STUDIO_LOG_SINK, StudioActor, StudioCommand,
    StudioController, StudioHandle, StudioLogSink, StudioSnapshot, StudioViewReceiver,
    StudioViewSender, UiConsoleView, UiError, UiLogDraft, UiLogEntry, UiLogLevel, UiLogOrigin,
    UiLogSource, UiNotice, UiNoticeLevel, UiResult, UxActivityTarget, UxUpdate, UxUpdateSink,
    ViewPublisher, studio_view_channel,
};
pub use core::notice::UiNotices;
pub use core::view::activity_view::UiActivityStep;
pub use core::view::activity_view::UiActivityStepState;
pub use core::view::steps_view::UiStepState;
pub use core::view::steps_view::UiStepView;
pub use core::{
    ActionClass, ActionConfirmation, ActionEnablement, ActionMeta, ActionPriority, Controller,
    ControllerContext, ControllerId, ControllerOp, PASSIVE_REFRESH_DEADLINE,
    PROJECT_ACTION_DEADLINE, PROJECT_EDITOR_ACTION_DEADLINE, PROJECT_LOAD_DEADLINE, UiAction,
    UiActions, UiActivityView, UiMetric, UiPaneAction, UiPaneView, UiProgress, UiStatus,
    UiStepsView, UiStudioView, UiTerminalLine, UiViewContent, UxNodePath,
};

pub const STUDIO_DEMO_PROJECT_ID: &str = "examples/basic";
