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
    ControlLayout2d, ControlPathSpan2d, ControlSampleEncoding, ControlSampleLayout,
    ControlSampleSpan, ExportFinding, ExportSeverity, LampType, LpFeature, LpValue, NodeId,
    NodeKind, PhasorConfig, PlayState, Revision, SlotMapKey, SlotPath, SlotPathSegment, ToLpValue,
    Waveform,
};

pub mod app;
pub mod controller;
pub mod core;

pub use self::core::status::UiStatusKind;
pub use lpc_history::{ContentHash, SyncRelation};

pub use self::core::issue::UiIssue;
pub use self::core::view::progress_state::ProgressState;
pub use app::agent::{
    AgentController, AgentCostRates, AgentEditRecord, AgentFeedback, AgentModelsFetchFuture,
    AgentOp, AgentProviderConfig, AgentRunContext, AgentSessionKey, AgentTaskFuture,
    AgentTimerFactory, AgentTimerFuture, AgentViewContext, MAX_EDIT_RECORDS, UiAgentAvailability,
    UiAgentDebugDump, UiAgentHistoryEntry, UiAgentModelView, UiAgentStatus, UiAgentToolRow,
    UiAgentTurn, UiAgentUsage, UiAgentView, instant_agent_timer,
};
pub use app::bus::{
    UiBusChannelPreview, UiBusChannelView, UiBusSiteOrigin, UiBusSiteView, UiBusView,
};
pub use app::device::{
    BootloaderEntryFlow, ConnectFlowState, ConnectedDeviceSummary, DEPLOY_NODE_ID, DeployOp,
    DeployTarget, DeviceController, DeviceOp, DeviceOpenOutcome, DeviceTarget, EndpointChoice,
    ProviderChoice, RecoveryInstructions, RecoveryStep, UiDeviceBackup,
};
pub use app::docs_host::DocsSimHost;
pub use app::home::{
    CardOp, CardOpPhase, CardSheet, CardUiOp, CardUiState, CardVerb, DEFAULT_STRIP_PIXELS,
    GenerateProjectError, GeneratedProject, HOME_NODE_ID, HomeDeviceEvidence, HomeOp,
    HomePoolEvidence, HomeSimEvidence, ProjectTemplate, SIM_CARD_KEY, SetupSession,
    UiCardConnection, UiDeviceCard, UiDeviceProjectChip, UiExampleCard, UiHomeView, UiPackageCard,
    UiSetupProject, UiSetupRailPhase, UiSetupRailStep, UiSetupWizard, ZipBytes,
    generate_board_project, setup_rail, template_project_files,
};
pub use app::node::{
    UiAssetEditor, UiAssetEditorKind, UiBindingAuthoring, UiBindingAuthoringDirection,
    UiBindingEndpoint, UiCellProjection, UiChannelChoice, UiClockFace, UiClockTransport,
    UiConfigSlot, UiConfigSlotBody, UiConsumerPolicy, UiControlProductPreview,
    UiControlSampleFormat, UiExportsGroup, UiFixtureFace, UiFixturePower, UiLedBudget,
    UiModuleExport, UiModuleFace, UiNodeChild, UiNodeDirtyState, UiNodeFace, UiNodeHeader,
    UiNodeSection, UiNodeTab, UiNodeTabBody, UiNodeView, UiOutputBoardFacts, UiOutputChannelRow,
    UiOutputFace, UiOutputPin, UiPanelControl, UiPanelControlState, UiPanelControlView,
    UiPanelEmit, UiPanelGroup, UiPanelTarget, UiPanelWidget, UiPanelWire, UiPanelWireRole,
    UiPhasorReading, UiPlaylistEntry, UiPlaylistFace, UiProducedBinding, UiProducedBindings,
    UiProducedProduct, UiProducedValue, UiProductKind, UiProductPreview, UiProductPreviewFrame,
    UiProductRef, UiProductSpaceView, UiProductTrackingState, UiProjectionOrigin, UiShaderFace,
    UiShaderUniform, UiSlotAffordance, UiSlotAspect, UiSlotAspectKind, UiSlotAspectRow,
    UiSlotAsset, UiSlotComposite, UiSlotEditorHint, UiSlotEnumComposite, UiSlotFieldState,
    UiSlotMapComposite, UiSlotMapKeyKind, UiSlotOption, UiSlotOptionality, UiSlotRecord,
    UiSlotShape, UiSlotShapeField, UiSlotSourceState, UiSlotUnit, UiSlotValue, UiSlotValueKind,
    UiSpaceCell, UiSpaceCellRole, UiSpaceChoice, UiSpaceFlag, UiSpaceFlagRole, UiSpaceMismatch,
    UiSpaceSection, UiSpaceSide, UiTimebaseState, UiVisualProductSpace, UiVisualSpace,
    UiWireStatus, phasor_rate_display,
};
#[cfg(all(feature = "browser-worker", target_arch = "wasm32"))]
pub use app::preview_host::{PreviewHost, PreviewSlotHandle};
pub use app::preview_host::{
    PreviewHostConfig, PreviewProfile, PreviewSlotRequest, PreviewSlotStatus, PreviewSource,
    PreviewTier, is_teardown_abort_reason,
};
pub use app::project::{
    AgentEngineStatus, AssetContentFetchOp, AssetEditOp, DirtySummary, LoadedProjectChoice,
    MAX_ASSET_BODY_BYTES, ModuleExportOp, ModuleHeroProduct, NodeCardDrawer, NodeCardUiState,
    NodeClearDebugOp, NodeController, NodeControllerState, NodeCopyOp, NodeCreateOp, NodeImportOp,
    NodePasteOp, NodeRemoveOp, NodeRevertOp, NodeUiOp, PanelAutoSaveOp, PanelClearOp, PanelWriteOp,
    PendingAssetEdit, PendingEdit, PendingEditOp, PendingEditPhase, PlaylistActivateOp,
    ProjectAssetContentRun, ProjectConnectResult, ProjectController, ProjectEditRun,
    ProjectEditorOp, ProjectEditorTarget, ProjectEditorView, ProjectInventorySummary,
    ProjectNodeAddress, ProjectNodeStatusTone, ProjectNodeStatusView, ProjectNodeTarget,
    ProjectNodeTreeItem, ProjectNodeTreeView, ProjectOp, ProjectProductSubscriptionIntent,
    ProjectRefreshOutcome, ProjectRuntimeSummary, ProjectSlotAddress, ProjectSlotRoot,
    ProjectSnapshot, ProjectState, ProjectSync, ProjectSyncPhase, ProjectSyncRun,
    ProjectSyncSummary, SlotController, SlotControllerState, SlotEditOp, SlotKind, UiAddNodeMenu,
    UiAddNodeMenuEntry, UiAffordance, UiAssetContent, UiAssetContentBody, UiAttachTarget,
    UiImportablePattern, UiNodeRemovePreflight, UiPendingEdit, UiPendingEditKind,
    UiPendingEditPhase, UiPreviewSpaces, UiProductSpaceRequest, UiProjectManifest, UiShaderError,
    UiTimebaseRead, visual_probe_request,
};
pub use app::rich_object::{
    RichChip, RichLine, RichObjectView, RichRollup, RichSection, RichWeight,
};
pub use app::roster::board_display_name;
pub use app::roster::{
    BundledFirmware, CardTabView, ConnectEvidence, ConnectPhase, DegradedReason, DeviceCardTab,
    DeviceDetailAffordance, DeviceFormatStanding, DeviceRichInput, RosterAffordance,
    RosterCardState, RosterEvidence, RosterStateSpec, RosterTreatment, SimDetailAffordance,
    SimRichInput, derive_roster_card_state, device_card_tabs, device_rich_object,
    firmware_update_available, sim_rich_object,
};
pub use app::runtime_pool::{
    CardFeedApply, CardFeedState, DEVICE_SESSION_CAPACITY, DeviceHandle, InstallRefusal, RuntimeId,
    RuntimeKind, RuntimePayload, RuntimePool, RuntimeSession, SIM_SESSION_CAPACITY, SimAttachment,
    SimLoadedProject,
};
pub use app::server::{
    LoadedDemoProject, LoadedProjectCatalog, ServerFailureKind, ServerOp, ServerSnapshot,
    ServerState, StudioCreateNode, StudioFsRead, StudioOverlayCommit, StudioOverlayMutation,
    StudioOverlayRead, StudioProjectRead, StudioProjectReadOutcome, StudioRemoveNode,
    StudioServerClient,
};
pub use app::settings::{
    AgentProvider, AgentProviderGuidance, AgentSettings, BrowserFacts, COMMON_LOCAL_SERVERS,
    DEFAULT_AGENT_MODEL, FindingKind, LocalModelProbeState, LocalServer, ProbeFinding, ProbeLevel,
    ProbeOutcome, ProbeSummary, SettingsCommand, SettingsLayer, SettingsStore, StudioSettings,
    UiAgentSettingsView, UiModelOption, UiSettingsView, provider_guidance,
};
pub use app::setup_flow::{
    BoardPickState, BoardProbe, BoardVerdict, CloseReason, ConnectHint, HardwareSetupTarget,
    ProbeEvidence, ProvisionPhase, ProvisionState, SetupCapabilities, SetupCommand, SetupContext,
    SetupDispatch, SetupEvent, SetupEventKind, SetupExecutorContext, SetupFlow, SetupGesture,
    SetupState, SetupStateKind, SetupStep, SetupTarget, SimulatorSetupTarget, classify_board,
    derive_device_name, dispatch_for, known_device_for, month_day_label, unique_device_name,
};
pub use app::share::{
    NODE_KIND, NodeEnvelope, PACKAGE_KIND, PackageEnvelope, SHARE_FORMAT_VERSION, ShareError,
    ShareFile, ShareHeader, peek_header,
};
pub use app::studio::{
    ConsoleCommand, DEVICE_CARD_FEED_INTERVAL, DEVICE_HEARTBEAT_INTERVAL, DEVICE_REFRESH_INTERVAL,
    FRAME_STALE_AFTER_SECS, LOG_RING_CAPACITY, LogClock, LogFilter, LogRing,
    PASSIVE_PREEMPTIONS_BEFORE_PROMOTION, RefreshCadence, SIMULATOR_REFRESH_INTERVAL,
    STUDIO_LOG_SINK, StudioActor, StudioActorOptions, StudioCommand, StudioController,
    StudioHandle, StudioLogSink, StudioSnapshot, StudioViewReceiver, StudioViewSender,
    UiChromeSession, UiChromeSessionStatus, UiChromeSessionTarget, UiConsoleView, UiError,
    UiLensRuntime, UiLogDraft, UiLogEntry, UiLogLevel, UiLogOrigin, UiLogSource, UiNotice,
    UiNoticeLevel, UiResult, UxActivityTarget, UxUpdate, UxUpdateSink, VERDICT_CHASE_INTERVAL,
    VERDICT_CHASE_TICKS, ViewPublisher, has_unsaved_work, studio_view_channel,
};
pub use core::notice::UiNotices;
pub use core::view::activity_view::UiActivityStep;
pub use core::view::activity_view::UiActivityStepState;
pub use core::{
    ActionClass, ActionConfirmation, ActionEnablement, ActionMeta, ActionPriority, Controller,
    ControllerContext, ControllerId, ControllerOp, DEVICE_CARD_FEED_CLASS,
    PASSIVE_REFRESH_DEADLINE, PROJECT_ACTION_DEADLINE, PROJECT_EDITOR_ACTION_DEADLINE,
    PROJECT_LOAD_DEADLINE, UiAction, UiActions, UiActivityView, UiMetric, UiPaneAction, UiPaneView,
    UiProgress, UiStatus, UiStudioView, UiTerminalLine, UiViewContent, UxNodePath,
};

pub const STUDIO_DEMO_PROJECT_ID: &str = "examples/fyeah-sign";
