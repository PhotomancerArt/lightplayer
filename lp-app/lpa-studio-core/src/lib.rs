//! Headless LightPlayer Studio application core.

/// The browser-serial connector's catalog-level granted-ports probe, for
/// the web shell's "has a device ever been granted here?" gate (the probe
/// FFI lives in lpa-link; stories stay prop-injected).
#[cfg(all(feature = "browser-serial-esp32", target_arch = "wasm32"))]
pub use lpa_link::providers::browser_serial_esp32::BrowserSerialEsp32Provider;
pub use lpa_link::{
    DeviceEvent, DeviceEventSink, DeviceLineOrigin, DeviceSession,
    DeviceSnapshot as LinkDeviceSnapshot, DeviceState, DeviceTimers, LinkEndpointId,
    LinkEndpointStatus, LinkProviderKind,
};
pub use lpc_model::{
    ArtifactLocation, ColorOrder, ControlDisplayLayout, ControlExtent, ControlLamp2d,
    ControlLayout2d, ControlSampleEncoding, ControlSampleLayout, ControlSampleSpan, LpValue,
    Revision, SlotMapKey, SlotPath, SlotPathSegment,
};

pub mod app;
pub mod controller;
pub mod core;

pub use self::core::status::UiStatusKind;
pub use lpc_history::{ContentHash, SyncRelation};

pub use self::core::issue::UiIssue;
pub use self::core::view::progress_state::ProgressState;
pub use app::bus::{UiBusChannelView, UiBusSiteView, UiBusView};
pub use app::device::{
    ConnectFlowState, ConnectedDeviceSummary, DEPLOY_NODE_ID, DeployOp, DeployState, DeployTarget,
    DeviceController, DeviceOp, DeviceOpenOutcome, EndpointChoice, ProviderChoice, UiDeployChoice,
    UiDeployView,
};
pub use app::home::{
    HOME_NODE_ID, HomeDeviceEvidence, HomeOp, UiCardConnection, UiDeviceCard, UiDeviceProjectChip,
    UiExampleCard, UiHomeView, UiPackageCard, ZipBytes,
};
pub use app::node::{
    UiAssetEditor, UiAssetEditorKind, UiBindingAuthoring, UiBindingAuthoringDirection,
    UiBindingEndpoint, UiChannelChoice, UiConfigSlot, UiConfigSlotBody, UiControlProductPreview,
    UiControlSampleFormat, UiNodeChild, UiNodeDirtyState, UiNodeHeader, UiNodeSection, UiNodeTab,
    UiNodeTabBody, UiNodeView, UiProducedBinding, UiProducedBindings, UiProducedProduct,
    UiProducedValue, UiProductKind, UiProductPreview, UiProductPreviewFrame, UiProductRef,
    UiProductTrackingState, UiShaderUniform, UiSlotAffordance, UiSlotAspect, UiSlotAspectKind,
    UiSlotAspectRow, UiSlotAsset, UiSlotComposite, UiSlotEditorHint, UiSlotEnumComposite,
    UiSlotFieldState, UiSlotMapComposite, UiSlotMapKeyKind, UiSlotOption, UiSlotOptionality,
    UiSlotRecord, UiSlotShape, UiSlotShapeField, UiSlotSourceState, UiSlotUnit, UiSlotValue,
    UiSlotValueKind,
};
#[cfg(all(feature = "browser-worker", target_arch = "wasm32"))]
pub use app::preview_host::{PreviewHost, PreviewSlotHandle};
pub use app::preview_host::{
    PreviewHostConfig, PreviewProfile, PreviewSlotRequest, PreviewSlotStatus, PreviewSource,
    PreviewTier,
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
pub use app::rich_object::{
    RichChip, RichLine, RichObjectView, RichRollup, RichSection, RichWeight,
};
pub use app::roster::{
    BundledFirmware, ConnectEvidence, ConnectPhase, DegradedReason, DeviceDetailAffordance,
    DeviceRichInput, RosterAffordance, RosterCardState, RosterCircle, RosterCircleShape,
    RosterEvidence, derive_roster_card_state, device_rich_object, firmware_update_available,
};
pub use app::runtime_pool::{
    DeviceHandle, RuntimeId, RuntimeKind, RuntimePayload, RuntimePool, RuntimeSession,
    SimAttachment,
};
pub use app::server::{
    LoadedDemoProject, LoadedProjectCatalog, ServerFailureKind, ServerOp, ServerSnapshot,
    ServerState, StudioFsRead, StudioOverlayCommit, StudioOverlayMutation, StudioOverlayRead,
    StudioProjectRead, StudioProjectReadOutcome, StudioServerClient,
};
pub use app::studio::{
    ConsoleCommand, DEVICE_REFRESH_INTERVAL, LOG_RING_CAPACITY, LogClock, LogFilter, LogRing,
    RefreshCadence, SIMULATOR_REFRESH_INTERVAL, STUDIO_LOG_SINK, StudioActor, StudioCommand,
    StudioController, StudioHandle, StudioLogSink, StudioSnapshot, StudioViewReceiver,
    StudioViewSender, UiConsoleView, UiError, UiLogDraft, UiLogEntry, UiLogLevel, UiLogOrigin,
    UiLogSource, UiNotice, UiNoticeLevel, UiResult, UxActivityTarget, UxUpdate, UxUpdateSink,
    VERDICT_CHASE_INTERVAL, VERDICT_CHASE_TICKS, ViewPublisher, studio_view_channel,
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

pub const STUDIO_DEMO_PROJECT_ID: &str = "examples/fyeah-sign";
