//! Headless LightPlayer Studio application core.

/// The browser-serial connector's catalog-level granted-ports probe, for
/// the web shell's "has a device ever been granted here?" gate (the probe
/// FFI lives in lpa-link; stories stay prop-injected).
#[cfg(all(feature = "browser-serial-esp32", target_arch = "wasm32"))]
pub use lpa_link::providers::browser_serial_esp32::BrowserSerialEsp32Provider;
pub use lpa_link::{LinkEndpointId, LinkEndpointStatus, LinkProviderKind};
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
#[cfg(all(feature = "browser-serial-esp32", target_arch = "wasm32"))]
pub use app::devices::BrowserSerialTransport;
pub use app::devices::{
    CompletedPush, DeviceEffectCall, DeviceEffectFacts, DeviceEffectProgress, DeviceEffects,
    DevicePushOp, DeviceRoster, DeviceRosterView, DeviceTaskFuture, DeviceTimerFuture,
    DeviceTransport, DeviceTransportFuture, DevicesOp, FlashBoardChoice, FlashOffer, GrantedLink,
    JournalLine, PushOffer, PushPayload, PushSource, PushSourceChoice, PushSourceGroup, StagedPush,
    device_escape_action, device_status_kind, first_bundled_example_id, flash_offer,
    pending_escape_action, push_offer,
};
pub use app::docs_host::DocsSimHost;
pub use app::home::{
    CardSheet, CardUiOp, CardUiState, CardVerb, DEFAULT_STRIP_PIXELS, GenerateProjectError,
    GeneratedProject, HOME_NODE_ID, HomeOp, HomePoolEvidence, HomeSimEvidence, ProjectTemplate,
    SIM_CARD_KEY, UiExampleCard, UiHomeView, UiPackageCard, UiSimCard, UiSimProjectChip, ZipBytes,
    generate_board_project, template_project_files,
};
pub use app::node::{
    UiAssetEditor, UiAssetEditorKind, UiBindingAuthoring, UiBindingAuthoringDirection,
    UiBindingEndpoint, UiCellProjection, UiChannelChoice, UiClockFace, UiClockTransport,
    UiConfigSlot, UiConfigSlotBody, UiConsumerPolicy, UiControlProductPreview,
    UiControlSampleFormat, UiExportsGroup, UiFixtureFace, UiFixturePatch, UiFixturePower,
    UiLedBudget, UiModuleExport, UiModuleFace, UiNodeChild, UiNodeDirtyState, UiNodeFace,
    UiNodeHeader, UiNodeSection, UiNodeTab, UiNodeTabBody, UiNodeView, UiOutputBoardFacts,
    UiOutputFace, UiOutputPin, UiOutputPortRow, UiPanelControl, UiPanelControlState,
    UiPanelControlView, UiPanelEmit, UiPanelGroup, UiPanelTarget, UiPanelWidget, UiPanelWire,
    UiPanelWireRole, UiPatchBay, UiPatchCell, UiPatchPort, UiPhasorReading, UiPlaylistEntry,
    UiPlaylistFace, UiProducedBinding, UiProducedBindings, UiProducedProduct, UiProducedValue,
    UiProductKind, UiProductPreview, UiProductPreviewFrame, UiProductRef, UiProductSpaceView,
    UiProductTrackingState, UiProjectionOrigin, UiProjectionShape, UiShaderFace, UiShaderUniform,
    UiShapePresets, UiSlotAffordance, UiSlotAspect, UiSlotAspectKind, UiSlotAspectRow, UiSlotAsset,
    UiSlotComposite, UiSlotEditorHint, UiSlotEnumComposite, UiSlotFieldState, UiSlotMapComposite,
    UiSlotMapKeyKind, UiSlotOption, UiSlotOptionality, UiSlotRecord, UiSlotShape, UiSlotShapeField,
    UiSlotSourceState, UiSlotUnit, UiSlotValue, UiSlotValueKind, UiSpaceBoolRow, UiSpaceCell,
    UiSpaceCellRole, UiSpaceChoice, UiSpaceMismatch, UiSpaceModifiers, UiSpaceSection, UiSpaceSide,
    UiTimebaseState, UiVisualProductSpace, UiVisualSpace, UiWireDirectionRow, UiWireStatus,
    phasor_rate_display,
};
pub use app::open_priority::{UserOpenGuard, begin_user_open, user_open_in_flight};
pub use app::open_progress::{
    OpenFailure, OpenStage, current_open_generation, note_open_requested, open_stage,
    open_superseded,
};
#[cfg(all(feature = "browser-worker", target_arch = "wasm32"))]
pub use app::preview_host::{PreviewHost, PreviewSlotHandle};
pub use app::preview_host::{
    PreviewHostConfig, PreviewPosterFrame, PreviewProfile, PreviewSlotRequest, PreviewSlotStatus,
    PreviewSource, PreviewTier, is_teardown_abort_reason,
};
pub use app::project::{
    AgentEngineStatus, AssetContentFetchOp, AssetEditOp, DirtySummary, EDIT_JOURNAL_CAP,
    EDITOR_META_PATH, EditorMetaFetchOp, EditorMetaFixture, EditorMetaOp, EditorMetaSet,
    EditorMetaVerb, FROZEN_PREVIEW_PHASE, LoadedProjectChoice, MAX_ASSET_BODY_BYTES,
    ModuleExportOp, ModuleHeroProduct, NodeCardDrawer, NodeCardUiState, NodeClearDebugOp,
    NodeController, NodeControllerState, NodeCopyOp, NodeCreateOp, NodeImportOp, NodePasteOp,
    NodeRemoveOp, NodeRevertOp, NodeUiOp, PanelAutoSaveOp, PanelClearOp, PanelWriteOp,
    PatchPulseLamps, PatchPulseLanguage, PatchPulseOp, PatchPulseSpace, PatchPulseSubject,
    PatchVerbFixture, PatchVerbKind, PatchVerbOp, PatchVerbSubject, PatchVerbWindow,
    PendingAssetEdit, PendingEdit, PendingEditOp, PendingEditPhase, PlaylistActivateOp,
    ProjectAssetContentRun, ProjectConnectResult, ProjectController, ProjectEditRun,
    ProjectEditorOp, ProjectEditorTarget, ProjectEditorView, ProjectInventorySummary,
    ProjectNodeAddress, ProjectNodeStatusTone, ProjectNodeStatusView, ProjectNodeTarget,
    ProjectNodeTreeItem, ProjectNodeTreeView, ProjectOp, ProjectProductSubscriptionIntent,
    ProjectRefreshOutcome, ProjectRuntimeSummary, ProjectSlotAddress, ProjectSlotRoot,
    ProjectSnapshot, ProjectState, ProjectSync, ProjectSyncPhase, ProjectSyncRun,
    ProjectSyncSummary, SlotController, SlotControllerState, SlotEditOp, SlotKind, UiAddNodeMenu,
    UiAddNodeMenuEntry, UiAffordance, UiArrangeFootprint, UiArrangeMeta, UiArrangeTransform,
    UiAssetContent, UiAssetContentBody, UiAttachTarget, UiEditJournalEntry, UiEditJournalEvent,
    UiEditorMode, UiImportablePattern, UiNodeRemovePreflight, UiPatchChasePreview, UiPatchInstance,
    UiPatchSurface, UiPatchSurfaceFixture, UiPatchSurfaceModule, UiPatchSurfaceOutput,
    UiPatchTarget, UiPendingEdit, UiPendingEditKind, UiPendingEditPhase, UiPreviewSpaces,
    UiProductSpaceRequest, UiProjectManifest, UiSelection, UiShaderError, UiTimebaseRead,
    chase_preview, editor_meta_artifact, preview_phase, visual_probe_request,
};
pub use app::rich_object::{
    RichChip, RichLine, RichObjectView, RichRollup, RichSection, RichWeight,
};
pub use app::roster::board_display_name;
pub use app::roster::{
    CardTab, CardTabView, SimCardState, SimDetailAffordance, SimRichInput, card_tabs,
    sim_rich_object,
};
pub use app::runtime_pool::{
    CardFeedApply, CardFeedState, DeviceLensAttachment, RuntimeId, RuntimeKind, RuntimeOp,
    RuntimePayload, RuntimePool, RuntimeSession, SIM_SESSION_CAPACITY, SimAttachment, SimLink,
    SimLoadedProject,
};
pub use app::server::{
    LoadedDemoProject, LoadedProjectCatalog, ServerFailureKind, ServerSnapshot, ServerState,
    StudioCreateNode, StudioFsRead, StudioOverlayCommit, StudioOverlayMutation, StudioOverlayRead,
    StudioProjectRead, StudioProjectReadOutcome, StudioRemoveNode, StudioServerClient,
};
pub use app::settings::{
    AgentProvider, AgentProviderGuidance, AgentSettings, BrowserFacts, COMMON_LOCAL_SERVERS,
    DEFAULT_AGENT_MODEL, FindingKind, LocalModelProbeState, LocalServer, ProbeFinding, ProbeLevel,
    ProbeOutcome, ProbeSummary, SettingsCommand, SettingsLayer, SettingsStore, StudioSettings,
    UiAgentSettingsView, UiModelOption, UiSettingsView, provider_guidance,
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
    UiChromeSessionControl, UiChromeSessionStatus, UiConsoleView, UiError, UiLensRuntime,
    UiLogDraft, UiLogEntry, UiLogLevel, UiLogOrigin, UiLogSource, UiNotice, UiNoticeLevel,
    UiResult, UxActivityTarget, UxUpdate, UxUpdateSink, VERDICT_CHASE_INTERVAL,
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
/// The device model's own vocabulary, re-exported so the web crate renders
/// and dispatches it without a second dependency edge. The model is the ONE
/// device vocabulary — there is no `Ui*` mirror of it, on purpose.
pub use lpa_devices::view::{
    ActivityView as DeviceActivityView, DeviceView, Escape as DeviceEscape,
    LoadedProject as DeviceLoadedProject, OutcomeView, PendingLinkView, RosterView,
};
pub use lpa_devices::{
    Action as DeviceAction, ActivityKind as DeviceActivityKind, DeviceId, DeviceStatus,
    EndpointKey as DeviceEndpointKey, Event as DeviceEvent, Input as DeviceInput,
    LinkId as DeviceLinkId, LinkInfo as DeviceLinkInfo, Millis as DeviceMillis,
    RosterConfig as DeviceRosterConfig,
};

pub const STUDIO_DEMO_PROJECT_ID: &str = "examples/fyeah-sign";
