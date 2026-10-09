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
    AGENT_ACTIVITY_KEPT, AGENT_ACTIVITY_LIT_SECS, AgentActivity, AgentActivityEntry,
    AgentActivityKind, UiAgentActivity, UiAgentLit, UiAgentPlace, UiAgentReveal,
};
pub use app::agent::{
    AgentController, AgentCostRates, AgentEditRecord, AgentFeedback, AgentModelsFetchFuture,
    AgentOp, AgentProviderConfig, AgentRunContext, AgentSessionKey, AgentTaskFuture,
    AgentTimerFactory, AgentTimerFuture, AgentViewContext, MAX_EDIT_RECORDS, UiAgentActPress,
    UiAgentAvailability, UiAgentCard, UiAgentCardState, UiAgentDebugDump, UiAgentEditBatch,
    UiAgentEditLine, UiAgentEditOutcome, UiAgentHistoryEntry, UiAgentModelView, UiAgentStatus,
    UiAgentToolRow, UiAgentTurn, UiAgentUsage, UiAgentView, UiAppAgentView, instant_agent_timer,
};
pub use app::bus::{
    UiBusChannelPreview, UiBusChannelView, UiBusSiteOrigin, UiBusSiteView, UiBusView,
};
#[cfg(all(feature = "browser-ble", target_arch = "wasm32"))]
pub use app::devices::BrowserBleSource;
#[cfg(all(feature = "emulator-tab", target_arch = "wasm32"))]
pub use app::devices::BrowserEmuLinkSource;
#[cfg(all(feature = "browser-websocket", target_arch = "wasm32"))]
pub use app::devices::BrowserLanSource;
#[cfg(all(feature = "browser-websocket", target_arch = "wasm32"))]
pub use app::devices::BrowserRelaySource;
#[cfg(all(feature = "browser-serial-esp32", target_arch = "wasm32"))]
pub use app::devices::BrowserSerialTransport;
#[cfg(all(feature = "browser-worker", target_arch = "wasm32"))]
pub use app::devices::BrowserSimLinkSource;
#[cfg(any(test, feature = "story-fixtures"))]
pub use app::devices::device_update_fixtures::{
    UpdateFixture, UpdateFixtureRow, file_build, looked_up_release,
};
pub use app::devices::{
    AUTOCONNECT_ENABLED_PARAM, BLE_ENDPOINT_PREFIX, Backing, BleDeviceTransport, BleLinkSource,
    BluetoothReach, BoardRef, BoardRefError, CompletedPush, CompositeDeviceTransport,
    DEVICE_FEED_PARK_AFTER_FAILURES, DEVICE_FRAME_SNAPSHOT_INTERVAL_SECS, DeviceCardFeedView,
    DeviceEffectCall, DeviceEffectFacts, DeviceEffectProgress, DeviceEffects, DeviceFace,
    DeviceFeedOp, DeviceFrameFeed, DeviceFrameFeeds, DeviceIdentityFirmware, DeviceIdentityLine,
    DeviceOfferFacts, DevicePushOp, DeviceRoster, DeviceRosterView, DeviceTaskFuture,
    DeviceTimerFuture, DeviceTransport, DeviceTransportFuture, DevicesOp, EMU_TRANSPORT,
    EmuBacking, EmuDeviceTransport, EmuLinkSource, EmuRuntimeControl, EmuSession,
    FLASH_ALL_BOARDS_PARAM, FLASH_BOARD_PARAM, FLASH_NAME_PARAM, FeedLiveness, FirmwareVerb,
    FlashBoardChoice, FlashOffer, GrantedLink, JournalLine, LAN_LINK_PATH, LanDeviceTransport,
    LanFlag, LanLinkSource, LensLineTap, LensTapEvent, NEW_SIM_BACKING_PARAM, NEW_SIM_BOARD_PARAM,
    NewSimRecord, PUSH_NAME_BOARD_PARAM, PUSH_NAME_PARAM, PUSH_SOURCE_PARAM,
    ProvisionalBoardNumbers, PushOffer, PushOver, PushPayload, PushSource, PushSourceChoice,
    PushSourceGroup, RENAME_NAME_PARAM, RESET_NEEDS_AUTHOR, RESET_WAITS_FOR_ANSWER, RememberedView,
    ResetReach, RosterSplit, RuntimeKind, SIM_TRANSPORT, SimBacking, SimCreateOp,
    SimDeviceTransport, SimLinkSource, SimRecord, SimRuntimeControl, SimSession, SimTier,
    StagedPush, TargetChoice, TargetGroup, TargetOffer, TargetScope, USB_NEEDS_WEB_SERIAL,
    UiLanLink, UiLinkKind, UiRuntimeBand, add_device_offers, backing_for, ble_endpoint,
    ble_link_info, blocked_erase_action, delete_sim_record, device_card_feed_view,
    device_card_feed_views, device_chip, device_escape_action, device_escape_action_for,
    device_firmware_line, device_id_from_ble_endpoint, device_identity_line, device_offers,
    device_status_kind, emu_endpoint, emu_link_info, emu_offered_for, escape_verb, feed_liveness,
    firmware_face_preview_sentence, firmware_verb, first_bundled_example_id, flash_consequence,
    flash_device_offer, flash_offer, flash_offer_for, flash_pending_offer, lan_link_for_endpoint,
    lan_link_view, mint_sim_identity, new_sim_offer, new_sim_record, normalize_lan_address,
    parse_lan_flag, pending_escape_action, pending_firmware_line, pending_identity_rows,
    pending_link_offers, push_device_offer, push_offer, read_sim_record, reflash_choice,
    sim_device_name, sim_endpoint, sim_link_info, split_roster, target_offer,
    transport_label_for_endpoint, uid_from_emu_endpoint, uid_from_sim_endpoint,
    update_firmware_offer, write_sim_record,
};
pub use app::devices::{
    BackupDownload, BackupEntry, BackupFileBytes, BackupIndex, BackupStatus, BackupStoreError,
    DeviceBackupOp, DeviceBackupStore, DeviceRestoreFromFileDataOp, DeviceRestoreFromFileOp,
    MemoryBackupStore, UiDeviceLayout, UiLayoutPanel, check_backup_file, check_store_contract,
    device_layout_view, device_restore_from_file_action,
};
pub use app::devices::{
    BundledOwnBuild, BundledOwnBuildSource, MemoryOwnBuildSource, OWN_BUILD_MISMATCH,
    OwnBuildSource, UpdateHost,
};
pub use app::devices::{DeviceFirmwareSources, StudioFirmwareStore};
pub use app::devices::{
    FIRST_BLUETOOTH_UPDATE_RELEASE, FirmwareFileDataOp, FirmwareFileOp, FirmwareLookupOp,
    INSTALL_FIND_PARAM, INSTALL_LIST_UNAVAILABLE, INSTALL_PRESS_LABEL, INSTALL_VERSION_PARAM,
    InstallChoice, PickedFirmwareFile, RECENT_CHOICES, StoreLatest, StoreLookup, StoreLookups,
    StoreReleases, USB_UPDATES_OVER_THE_AIR, UiDeviceUpdate, UiSessionUpdate, UpdateBuildFacts,
    UpdateLight, UpdateLink, UpdateOfferFacts, UpdateOffers, UpdateProgress, UpdateRoute,
    UpdateRowKind, UpdateRunTone, UpdateRunWord, UpdateStanding, UpdateStandingInputs,
    UpdateVersion, UpdateVersionDisplay, firmware_file_action, update_offers, update_route,
    update_session_words, update_standing, update_words, wants_auto_start,
};
pub use app::devices::{LINK_COUNTERS_CAPTION, LinkTrouble, UiLinkCounterRow, link_counter_rows};
pub use app::devices::{
    RELAY_CONNECTING, RELAY_NO_HELD_KEY_WORDS, RELAY_OFFLINE_WORDS, RELAY_UNREACHABLE_WORDS,
    RelayConnectFailure, RelayConnectOp, RelayDeviceTransport, RelayLinkSource,
    connect_relay_offer,
};
pub use app::devices::{
    UiWifiConnect, WIFI_ADDRESS_PARAM, WIFI_ADDRESSES_STORAGE_KEY, WIFI_BLOCKED_WORDS,
    WIFI_BUSY_WORDS, WIFI_CONNECTING, WIFI_NEEDS_WEBSOCKET, WifiAddress, WifiAddressBook,
    WifiAddressReach, WifiConnectFailure, WifiConnectOp, WifiConnectTarget, WifiConnects,
    connect_wifi_offer,
};
pub use app::docs_host::DocsSimHost;
pub use app::studio::PlayViewOp;
pub use app::studio::{UiPage, UiPanel, UiPlace, UiProjectView, UiSessionSection};
/// A board's MAC, as the roster records it.
pub use lpa_devices::identity::MacAddress;
// The project's declared hardware (D41): the web shell's Hardware row and
// the gallery card's "for <board>" badge both read it.
pub use app::access::{
    AccessAdded, AccessCommand, AccessPersist, AccessTier, AccountKeys, BrowserKey,
    DEFAULT_KDF_ITERATIONS, DeviceAccessChange, DroppedKey, MAX_SECRETS_PER_FILE, NetworkLinkKeys,
    OpenTo, PLAY_ONLY_SENTENCE, SecretKind, UiAccessPanel, UiDeviceAccess, UiKeyGroup,
    UiLoginPrompt, UiPasswordLine, UiUnlockOffer, account_key_refused_sentence, dropped_sentence,
    not_permitted_sentence, open_summary, tier_word,
};
pub use app::frame_feed::{
    CLOSE_INSPECTION_SAMPLE_FORMAT, CardFeedApply, CardFeedState, PREVIEW_SAMPLE_FORMAT,
};
pub use app::home::{
    DEFAULT_STRIP_PIXELS, GenerateProjectError, GeneratedProject, HOME_NODE_ID, HomeOp,
    NEW_PROJECT_NAME_PARAM, NEW_PROJECT_TEMPLATE_PARAM, OPEN_PROJECT_PARAM, ProjectTemplate,
    UiExampleCard, UiExampleGroup, UiHomeView, UiOpenMismatch, UiPackageCard, UiRunningProject,
    ZipBytes, example_groups, generate_board_project, home_offers, new_project_offer,
    open_project_offer, template_project_files,
};
pub use app::library::{DESKTOP_BOARD_ID, ProjectTarget};
pub use app::network::{
    NEEDS_AUTHOR, NetworkChange, NetworkCommand, NetworkOp, PasswordChange, READING, UiDeviceWifi,
    UiWifiNetworkRow, UiWifiTest, UiWifiTestResult, UiWifiTestStepLine, WIFI_ENABLED_PARAM,
    WIFI_FORGET_SEGMENT, WIFI_HIDDEN_PARAM, WIFI_NETWORK_PARAM, WIFI_PASSWORD_PARAM, WifiStepState,
    WifiTestNext, WifiTestOutcome, WifiTestProgress, WifiTestStep, WifiTone, signal_bars,
    signal_word,
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
    UiPanelWireRole, UiPatchBay, UiPatchCell, UiPatchPort, UiPatternEntryState, UiPatternPicker,
    UiPatternPickerEntry, UiPhasorReading, UiPlaylistEntry, UiPlaylistFace, UiProducedBinding,
    UiProducedBindings, UiProducedProduct, UiProducedValue, UiProductKind, UiProductPreview,
    UiProductPreviewFrame, UiProductRef, UiProductSpaceView, UiProductTrackingState,
    UiProjectionOrigin, UiProjectionShape, UiShaderFace, UiShaderUniform, UiShapePresets,
    UiSlotAffordance, UiSlotAspect, UiSlotAspectKind, UiSlotAspectRow, UiSlotAsset,
    UiSlotComposite, UiSlotEditorHint, UiSlotEnumComposite, UiSlotFieldState, UiSlotMapComposite,
    UiSlotMapKeyKind, UiSlotOption, UiSlotOptionality, UiSlotRecord, UiSlotShape, UiSlotShapeField,
    UiSlotSourceState, UiSlotUnit, UiSlotValue, UiSlotValueKind, UiSpaceBoolRow, UiSpaceCell,
    UiSpaceCellRole, UiSpaceChoice, UiSpaceMismatch, UiSpaceModifiers, UiSpaceSection, UiSpaceSide,
    UiTimebaseState, UiVisualProductSpace, UiVisualSpace, UiWireDirectionRow, UiWireStatus,
    phasor_rate_display,
};
pub use app::open_priority::{UserOpenGuard, begin_user_open, user_open_in_flight};
pub use app::open_progress::{
    DeviceOpenProgress, DeviceOpenStep, DeviceWait, DeviceWaitReason, OpenDevice, OpenFailure,
    OpenStage, cancel_open, current_open_generation, note_open_requested, open_stage,
    open_stage_label, open_superseded, record_open_stages,
};
#[cfg(all(feature = "browser-worker", target_arch = "wasm32"))]
pub use app::preview_host::{PreviewHost, PreviewSlotHandle};
pub use app::preview_host::{
    PreviewHostConfig, PreviewPosterFrame, PreviewProfile, PreviewSlotRequest, PreviewSlotStatus,
    PreviewSource, PreviewTier, is_teardown_abort_reason,
};
pub use app::project::{
    ADD_NODE_KIND_PARAM, ADD_NODE_VERB, ARRANGE_GROUP, ARRANGE_REDO_VERB, ARRANGE_ROTATION_PARAM,
    ARRANGE_SCALE_PARAM, ARRANGE_SET_VERB, ARRANGE_UNDO_VERB, ARRANGE_X_PARAM, ARRANGE_Y_PARAM,
    ASK_AGENT_REQUEST_PARAM, ASK_AGENT_VERB, AgentEngineStatus, AssetContentFetchOp, AssetEditOp,
    CLEAR_DEBUG_VERB, COPY_NODE_VERB, DirtySummary, EDIT_JOURNAL_CAP, EDITOR_META_PATH,
    EditorMetaFetchOp, EditorMetaFixture, EditorMetaOp, EditorMetaSet, EditorMetaVerb,
    FROZEN_PREVIEW_PHASE, HISTORY_ROW_CAP, IMPORT_BUILTIN_SECTION, IMPORT_LIBRARY_SECTION,
    IMPORT_PATTERN_PARAM, IMPORT_PATTERN_VERB, ImportSource, LoadedProjectChoice,
    MAX_ASSET_BODY_BYTES, ModuleExportOp, ModuleHeroProduct, NodeCardDrawer, NodeCardUiState,
    NodeClearDebugOp, NodeController, NodeControllerState, NodeCopyOp, NodeCreateOp, NodeImportOp,
    NodePasteOp, NodeRemoveOp, NodeRevertOp, NodeUiOp, PASTE_NODE_CLIPBOARD_PARAM, PASTE_NODE_VERB,
    PATCH_ASSIGN_VERB, PATCH_CLEAR_VERB, PATCH_DELTA_PARAM, PATCH_FLOW_AUTO, PATCH_FLOW_MANUAL,
    PATCH_FLOW_PARAM, PATCH_GROUP, PATCH_LAMP_PARAM, PATCH_LAMPS_PARAM, PATCH_OUTPUT_PARAM,
    PATCH_PORT_PARAM, PATCH_RE_ANCHOR_VERB, PATCH_REDO_VERB, PATCH_REVERSE_VERB, PATCH_ROTATE_VERB,
    PATCH_SET_FLOW_VERB, PATCH_SHIFT_PORT_VERB, PATCH_START_PARAM, PATCH_STEPS_PARAM,
    PATCH_SUBJECT_PARAM, PATCH_SWAP_PORTS_VERB, PATCH_UNDO_VERB, PATCH_UNMAP_ALL_VERB,
    PATCH_WHOLE_FIXTURE, PATCH_WITH_PARAM, PLAYLIST_CYCLE_VERB, PLAYLIST_CYCLING_PARAM,
    PLAYLIST_ENTRY_PARAM, PLAYLIST_NEXT_VERB, PLAYLIST_PLAY_VERB, PLAYLIST_PREV_VERB,
    PLAYLIST_SKIP_VERB, PLAYLIST_SKIPPED_PARAM, PLAYLIST_STEP_LONGER_VERB,
    PLAYLIST_STEP_SHORTER_VERB, PanelAutoSaveOp, PanelClearOp, PanelWriteOp, PatchPulseLamps,
    PatchPulseLanguage, PatchPulseOp, PatchPulseSpace, PatchPulseSubject, PatchVerbFixture,
    PatchVerbKind, PatchVerbOp, PatchVerbSubject, PatchVerbWindow, PendingAssetEdit, PendingEdit,
    PendingEditOp, PendingEditPhase, PlaylistActivateOp, ProjectAssetContentRun,
    ProjectConnectResult, ProjectController, ProjectEditRun, ProjectEditorOp, ProjectEditorTarget,
    ProjectEditorView, ProjectInventorySummary, ProjectNodeAddress, ProjectNodeStatusTone,
    ProjectNodeStatusView, ProjectNodeTarget, ProjectNodeTreeItem, ProjectNodeTreeView, ProjectOp,
    ProjectProductSubscriptionIntent, ProjectRefreshOutcome, ProjectRuntimeSummary,
    ProjectSlotAddress, ProjectSlotRoot, ProjectSnapshot, ProjectState, ProjectSync,
    ProjectSyncPhase, ProjectSyncRun, ProjectSyncSummary, REVERT_EDIT_PARAM, REVERT_EDIT_VERB,
    SAVE_COPY_VERB, SlotController, SlotControllerState, SlotEditOp, SlotKind, UiAddNodeMenu,
    UiAddNodeMenuEntry, UiAffordance, UiArrangeFootprint, UiArrangeMeta, UiArrangeTransform,
    UiAssetContent, UiAssetContentBody, UiAttachTarget, UiEditJournalEntry, UiEditJournalEvent,
    UiEditorMode, UiHistoryKind, UiImportablePattern, UiNodeRemovePreflight, UiPatchChasePreview,
    UiPatchInstance, UiPatchSurface, UiPatchSurfaceFixture, UiPatchSurfaceModule,
    UiPatchSurfaceOutput, UiPatchTarget, UiPendingEdit, UiPendingEditKind, UiPendingEditPhase,
    UiPreviewSpaces, UiProductSpaceRequest, UiProjectHistory, UiProjectHistoryEntry,
    UiProjectManifest, UiSelection, UiShaderError, UiTimebaseRead, arrange_batch,
    arrange_history_path, asset_body_too_large, chase_preview, editor_meta_artifact,
    is_header_verb, patch_history_path, preview_phase, publish_arrange_offers,
    publish_patch_verb_offers, revert_edit_offer, visual_probe_request,
};
pub use app::rich_object::{
    RichChip, RichLine, RichObjectView, RichRollup, RichSection, RichWeight,
};
pub use app::roster::board_display_name;
pub use app::runtime_pool::{
    DeviceLensAttachment, LinkTransport, RuntimeId, RuntimeOp, RuntimePayload, RuntimePool,
    RuntimeSession, SESSION_CAPACITY,
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
    UiAgentSettingsView, UiDeviceSettingsView, UiModelOption, UiSettingsView, provider_guidance,
};
pub use app::share::{
    NODE_KIND, NodeEnvelope, PACKAGE_KIND, PackageEnvelope, SHARE_FORMAT_VERSION, ShareError,
    ShareFile, ShareHeader, peek_header,
};
pub use app::studio::{
    ConsoleCommand, DEVICE_CARD_FEED_BLE_INTERVAL, DEVICE_CARD_FEED_INTERVAL,
    DEVICE_HEARTBEAT_INTERVAL, DEVICE_REFRESH_INTERVAL, FRAME_STALE_AFTER_SECS, LOG_RING_CAPACITY,
    LogClock, LogFilter, LogRing, PASSIVE_PREEMPTIONS_BEFORE_PROMOTION, RefreshCadence,
    SIMULATOR_REFRESH_INTERVAL, STUDIO_LOG_SINK, StudioActor, StudioActorOptions, StudioCommand,
    StudioController, StudioHandle, StudioLogSink, StudioSnapshot, StudioViewReceiver,
    StudioViewSender, UiChromeSessionControl, UiChromeSessionStatus, UiConsoleView, UiError,
    UiLensCard, UiLensReconnecting, UiLensRuntime, UiLogDraft, UiLogEntry, UiLogLevel, UiLogOrigin,
    UiLogSource, UiNotice, UiNoticeLevel, UiResult, UxActivityTarget, UxUpdate, UxUpdateSink,
    VERDICT_CHASE_INTERVAL, VERDICT_CHASE_TICKS, ViewPublisher, has_unsaved_work,
    set_device_lens_pause_override, studio_view_channel,
};
pub use core::log::{DeviceEventKind, DeviceEventRecorder};
pub use core::notice::UiNotices;
pub use core::offer::{
    FILTER_FINDS_NOTHING, OfferArgError, OfferArgs, OfferBinder, OfferChoice, OfferNearness,
    OfferParam, OfferParamKind, OfferPath, OfferPathError, OfferPress, SECRET_MARKER, UiOffer,
    UiOfferFocus, UiOfferTree,
};
pub use core::view::activity_view::UiActivityStep;
pub use core::view::activity_view::UiActivityStepState;
pub use core::{
    ActionClass, ActionConfirmation, ActionConsequence, ActionEnablement, ActionMeta,
    ActionPriority, Controller, ControllerContext, ControllerId, ControllerOp,
    DEVICE_CARD_FEED_CLASS, PASSIVE_REFRESH_DEADLINE, PROJECT_ACTION_DEADLINE,
    PROJECT_EDITOR_ACTION_DEADLINE, PROJECT_LOAD_DEADLINE, UiAction, UiActions, UiActivityView,
    UiMetric, UiPaneView, UiProgress, UiStatus, UiStudioView, UiTerminalLine, UiViewContent,
    UxNodePath,
};
/// The device model's own vocabulary, re-exported so the web crate renders
/// and dispatches it without a second dependency edge. The model is the ONE
/// device vocabulary — there is no `Ui*` mirror of it, on purpose.
pub use lpa_devices::view::{
    ActivityView as DeviceActivityView, DeviceView, Escape as DeviceEscape, FIRMWARE_NEEDS_USB,
    FirmwareFace as DeviceFirmwareFace, LoadedProject as DeviceLoadedProject, OutcomeView,
    PendingLinkView, RosterView,
};
pub use lpa_devices::wire::BoardFs as DeviceBoardFs;
pub use lpa_devices::{
    Action as DeviceAction, ActivityKind as DeviceActivityKind, AppVersion as DeviceAppVersion,
    BoardKey, DeviceId, DeviceStatus, EndpointKey as DeviceEndpointKey, Event as DeviceEvent,
    FirmwareAge as DeviceFirmwareAge, FlashLayoutView as DeviceFlashLayoutView,
    FlashStep as DeviceFlashStep, Input as DeviceInput, LayoutVerdict as DeviceLayoutVerdict,
    LinkCounterFacts as DeviceLinkCounters, LinkId as DeviceLinkId, LinkInfo as DeviceLinkInfo,
    Millis as DeviceMillis, RosterConfig as DeviceRosterConfig, TerminalKind as DeviceTerminalKind,
    TerminalLine as DeviceTerminalLine, WireVersion as DeviceWireVersion,
};
/// What a board reports about its saved networks, its station and what it
/// hears.
pub use lpc_wire::server::{
    HeardNetwork, LastAttempt, NetworkStatus, SavedNetworkInfo, StationFailure, StationState,
};

pub const STUDIO_DEMO_PROJECT_ID: &str = "catalog/fyeah-sign";

/// This Studio's own app version: `2026.10.03-1` for a tagged release, the
/// dev form `<short-sha>[-dirty-<HHMMSS>PT]` otherwise, stamped at build time
/// by the one helper every versioned build uses (`tools/lp-app-version`).
/// A board's hello version is compared against it to say "older than
/// Studio" ([`DeviceFirmwareAge`]).
pub const STUDIO_VERSION: &str = env!("LP_APP_VERSION");
