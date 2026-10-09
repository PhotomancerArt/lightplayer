//! The device layer's app half: the effects that execute `lpa-devices`'
//! [`Command`](lpa_devices::Command)s, and the sub-controller that owns the
//! [`Roster`](lpa_devices::Roster).
//!
//! The model is sans-IO by construction — it emits commands and forgets them
//! — so everything with a clock, a port, a filesystem or an executor in it
//! lives here:
//!
//! | `Command` | executed by |
//! |---|---|
//! | `Link { command }` | [`DeviceEffects`] → the routed [`Link`](lpa_devices::Link) (browser Web Serial or the tab emulator on wasm, the fake on the host) |
//! | `StartTimer` | [`DeviceEffects`] → one spawned future per timer on the app's timer factory |
//! | `PersistRecord` / `DeleteRecord` | [`DeviceRoster`] → the kept `places::device_registry`, through the library host's locked catalog |
//! | `RequestUsbGrant` | [`DeviceTransport::request_grant`] → the platform chooser |
//! | `RequestBleGrant` | [`DeviceTransport::request_ble_grant`] → the platform's Bluetooth chooser |
//! | `RevokeGrant` | [`DeviceTransport::revoke_grant`] → the provider's `forget_endpoint` |
//! | `RunEffect` | [`DeviceEffects::run_effect`] → the wire, borrowed exclusively: esptool for a flash, the `lpa-client` conversation for a push |
//!
//! Not a command — frames are not evidence — but the same store: a fed
//! board's newest picture is written to `/device-frames/<uid>.json`
//! ([`device_frame_snapshot`]) at most every ten seconds, and read back
//! into its feed at library settle so a remembered board keeps its last
//! picture across reloads.
//!
//! # Invariant I7: the fold loop never awaits device IO
//!
//! Every link event reaches the model the same way a user gesture does — as a
//! [`StudioCommand`](crate::StudioCommand) on the actor's ordered queue. A
//! spawned pump future per link drains
//! [`Link::poll_event`](lpa_devices::Link::poll_event) (which never blocks)
//! and enqueues; the actor's fold is a synchronous
//! [`Roster::handle`](lpa_devices::Roster::handle) call. Nothing in the fold
//! path awaits a port, which is what kills the wedged-page class.
//!
//! The `#[cfg(test)]` `Bench` harness in `lpa-link`'s `device_link::tests` is
//! the miniature of this module; the discipline (drain the wire, then the due
//! timers, generation-stamped) is the same.

/// The Bluetooth transport (M5): a control-only link, host-tested through
/// its source seam.
pub mod add_device_offers;
pub mod ble_transport;
pub mod bluetooth_reach;
/// One tab holds a board: the hold vocabulary, the book, the edge trait and
/// its in-memory double.
pub mod board_hold;
pub mod board_plays;
pub mod board_projects;
pub mod board_ref;
/// Bluetooth devices backed by the page's Web Bluetooth. wasm-only, and only
/// when the studio is built with the provider that owns them.
#[cfg(all(feature = "browser-ble", target_arch = "wasm32"))]
pub mod browser_ble_source;
/// Emus backed by the tab's emulator Worker. wasm-only, and only when the
/// studio is built with the module that owns them.
#[cfg(all(feature = "emulator-tab", target_arch = "wasm32"))]
pub mod browser_emu_source;
#[cfg(all(feature = "browser-websocket", target_arch = "wasm32"))]
pub mod browser_lan_source;
#[cfg(all(feature = "browser-websocket", target_arch = "wasm32"))]
pub mod browser_relay_source;
/// Sims backed by `fw-browser` workers. wasm-only, and only when the studio
/// is built with the provider that owns them.
#[cfg(all(feature = "browser-worker", target_arch = "wasm32"))]
pub mod browser_sim_source;
/// The browser Web Serial transport. wasm-only, and only when the studio is
/// built with the provider that owns the port.
#[cfg(all(feature = "browser-serial-esp32", target_arch = "wasm32"))]
pub mod browser_transport;
pub mod bundled_own_build;
pub mod composite_transport;
pub mod device_affordance;
pub mod device_backup_import;
pub mod device_backup_op;
pub mod device_backup_store;
pub mod device_by_base_mac;
pub mod device_card_feed_view;
pub mod device_effects;
pub mod device_feed_op;
pub mod device_firmware_face;
pub mod device_firmware_sources;
pub mod device_flash;
pub mod device_flash_offer;
pub mod device_frame_feed;
pub mod device_frame_snapshot;
pub mod device_identity;
pub mod device_layout_effect;
pub mod device_layout_step;
pub mod device_layout_view;
pub mod device_link_counter_rows;
pub mod device_offers;
pub mod device_push;
pub mod device_push_offer;
pub mod device_records;
pub mod device_reset_reach;
pub mod device_roster;
pub mod device_transport;
/// The update story's inputs, built for tests and for the web's stories.
#[cfg(any(test, feature = "story-fixtures"))]
pub mod device_update_fixtures;
pub mod device_update_offers;
pub mod device_update_route;
pub mod device_update_standing;
pub mod device_update_version;
pub mod device_update_words;
pub mod devices_op;
pub mod emu_transport;
pub mod firmware_file_build;
pub mod firmware_lookup_op;
pub mod install_choice;
pub mod lan_addresses;
pub mod lan_link_view;
pub mod lan_transport;
pub mod link_health;
pub mod new_sim_offer;
pub mod own_build_source;
pub mod pending_link_offers;
pub mod provisional_board_numbers;
pub mod relay_connect_failure;
pub mod relay_connect_offer;
pub mod relay_connect_op;
pub mod relay_transport;
pub mod runtime_backing;
pub mod runtime_band;
pub mod shared_link_client_io;
pub mod sim_create_op;
pub mod sim_record;
pub mod sim_transport;
pub mod store_lookups;
pub mod take_over_offer;
pub mod take_over_op;
pub mod take_over_state;
pub mod target_offer;
pub mod ui_link_kind;
pub(crate) mod update_auto_start;
pub mod update_build_facts;
pub(crate) mod update_driver_mirror;
pub mod update_host;
pub(crate) mod update_narration;
pub(crate) mod update_store_builds;
pub mod wifi_address_book;
pub mod wifi_connect_failure;
pub mod wifi_connect_offer;
pub mod wifi_connect_op;
pub mod wifi_connects;
pub mod wire_conversation;

pub use add_device_offers::{
    USB_NEEDS_WEB_SERIAL, WIFI_ADDRESS_PARAM, WIFI_CONNECTING, WIFI_NEEDS_WEBSOCKET,
    WifiAddressReach, add_device_offers,
};
pub use ble_transport::{BleDeviceTransport, BleLinkSource};
pub use bluetooth_reach::BluetoothReach;
pub use board_hold::{
    AskOutcome, AskRefusal, BoardHoldBook, BoardHoldEdge, BookChange, ClaimAnswer,
    HOLD_PROTO_VERSION, HoldEdgeEvent, HoldKey, HoldNote, MemoryBoardHold, MemoryBoardHoldBus,
    OtherHold, PendingAsk, TabId, UsbPair,
};
pub use board_plays::BoardPlays;
pub use board_projects::{BoardProjectInputs, BoardProjects, board_projects};
pub use board_ref::{BoardRef, BoardRefError};
#[cfg(all(feature = "browser-ble", target_arch = "wasm32"))]
pub use browser_ble_source::BrowserBleSource;
#[cfg(all(feature = "emulator-tab", target_arch = "wasm32"))]
pub use browser_emu_source::BrowserEmuLinkSource;
#[cfg(all(feature = "browser-websocket", target_arch = "wasm32"))]
pub use browser_lan_source::BrowserLanSource;
#[cfg(all(feature = "browser-websocket", target_arch = "wasm32"))]
pub use browser_relay_source::BrowserRelaySource;
#[cfg(all(feature = "browser-worker", target_arch = "wasm32"))]
pub use browser_sim_source::BrowserSimLinkSource;
#[cfg(all(feature = "browser-serial-esp32", target_arch = "wasm32"))]
pub use browser_transport::BrowserSerialTransport;
pub use bundled_own_build::{BundledOwnBuild, BundledOwnBuildSource, OWN_BUILD_MISMATCH};
pub use composite_transport::CompositeDeviceTransport;
pub use device_affordance::{
    device_escape_action, device_escape_action_for, device_status_kind, pending_escape_action,
};
pub use device_backup_import::{
    BackupFileBytes, DeviceRestoreFromFileDataOp, DeviceRestoreFromFileOp, check_backup_file,
    device_restore_from_file_action,
};
pub use device_backup_op::DeviceBackupOp;
pub use device_backup_store::{
    BackupEntry, BackupIndex, BackupStatus, BackupStoreError, DeviceBackupStore, MemoryBackupStore,
    check_store_contract,
};
pub use device_by_base_mac::{DeviceByBaseMac, device_by_base_mac};
pub use device_card_feed_view::{
    DeviceCardFeedView, FeedLiveness, device_card_feed_view, device_card_feed_views, feed_liveness,
};
pub use device_effects::{
    CompletedPush, DeviceEffects, DeviceTaskFuture, DeviceTimerFuture, PendingWrites, PushPayload,
    StagedPush,
};
pub use device_feed_op::DeviceFeedOp;
pub use device_firmware_face::{
    device_firmware_line, firmware_face_preview_sentence, pending_firmware_line,
};
pub use device_firmware_sources::{DeviceFirmwareSources, StudioFirmwareStore};
pub use device_flash::{
    FirmwareVerb, FlashBoardChoice, FlashOffer, blocked_erase_action, derive_flash_name,
    firmware_verb, flash_offer, flash_offer_for, reflash_choice, taken_device_titles,
};
pub use device_flash_offer::{
    FLASH_ALL_BOARDS_PARAM, FLASH_BOARD_PARAM, FLASH_NAME_PARAM, flash_consequence,
    flash_device_offer, flash_pending_offer, update_firmware_offer,
};
pub use device_frame_feed::{DEVICE_FEED_PARK_AFTER_FAILURES, DeviceFrameFeed, DeviceFrameFeeds};
pub use device_frame_snapshot::DEVICE_FRAME_SNAPSHOT_INTERVAL_SECS;
pub use device_identity::{
    DeviceIdentityLine, IdentityFirmware as DeviceIdentityFirmware, IdentityRows, device_chip,
    device_identity_line, pending_identity_rows,
};
pub use device_layout_effect::BackupDownload;
pub use device_layout_view::{UiDeviceLayout, UiLayoutPanel, device_layout_view};
pub use device_link_counter_rows::{LINK_COUNTERS_CAPTION, UiLinkCounterRow, link_counter_rows};
pub use device_offers::{
    AUTOCONNECT_ENABLED_PARAM, DeviceOfferFacts, RENAME_NAME_PARAM, device_offers, escape_verb,
};
pub use device_push::{
    DevicePushOp, PushOffer, PushSource, PushSourceChoice, PushSourceGroup,
    first_bundled_example_id, push_offer,
};
pub use device_push_offer::{
    PUSH_NAME_BOARD_PARAM, PUSH_NAME_PARAM, PUSH_SOURCE_PARAM, PushOver, push_device_offer,
};
pub use device_records::{
    EMU_TRANSPORT, SIM_TRANSPORT, auto_record_name, record_from_registry_row,
    registry_row_from_record, transport_label_for_endpoint,
};
pub use device_reset_reach::{RESET_NEEDS_AUTHOR, RESET_WAITS_FOR_ANSWER, ResetReach};
pub use device_roster::{
    DeviceRoster, DeviceRosterView, JournalLine, RememberedView, RosterSplit, split_roster,
};
pub use device_transport::{
    DeviceEffectCall, DeviceEffectFacts, DeviceEffectProgress, DeviceTransport,
    DeviceTransportFuture, GrantedLink, LensLineTap, LensTapEvent,
};
pub use device_update_offers::{
    INSTALL_FIND_PARAM, INSTALL_LIST_UNAVAILABLE, INSTALL_PRESS_LABEL, INSTALL_VERSION_PARAM,
    LookupStand, UpdateOfferFacts, UpdateOffers, update_offers,
};
pub use device_update_route::{
    FIRST_BLUETOOTH_UPDATE_RELEASE, USB_UPDATES_OVER_THE_AIR, UpdateLink, UpdateRoute, update_route,
};
pub use device_update_standing::{
    UpdateStanding, UpdateStandingInputs, update_standing, wants_auto_start,
};
pub use device_update_version::{UpdateVersion, UpdateVersionDisplay};
pub use device_update_words::{
    UiDeviceUpdate, UiSessionUpdate, UpdateLight, UpdateProgress, UpdateRowKind, UpdateRunTone,
    UpdateRunWord, update_session_words, update_words,
};
pub use devices_op::{DeviceFace, DevicesOp};
pub use emu_transport::{
    EmuBacking, EmuDeviceTransport, EmuLinkSource, EmuRuntimeControl, EmuSession,
};
pub use firmware_file_build::{
    FIRMWARE_FILE_MANIFEST, FirmwareFileBuild, FirmwareFileDataOp, FirmwareFileOp,
    PickedFirmwareFile, firmware_file_action, read_firmware_files,
};
pub use firmware_lookup_op::FirmwareLookupOp;
pub use install_choice::{
    InstallChoice, InstallChoiceInputs, RECENT_CHOICES, index_for, install_choices,
};
pub use lan_addresses::{LAN_LINK_PATH, LanFlag, normalize_lan_address, parse_lan_flag};
pub use lan_link_view::{UiLanLink, lan_link_for_endpoint, lan_link_view};
pub use lan_transport::{LanDeviceTransport, LanLinkSource};
pub use link_health::{LinkHealth, LinkHealthMap, LinkTrouble};
pub use new_sim_offer::{NEW_SIM_BACKING_PARAM, NEW_SIM_BOARD_PARAM, new_sim_offer};
pub use own_build_source::{MemoryOwnBuildSource, OwnBuildSource};
pub use pending_link_offers::pending_link_offers;
pub use provisional_board_numbers::ProvisionalBoardNumbers;
pub use relay_connect_failure::{
    RELAY_NO_HELD_KEY_WORDS, RELAY_OFFLINE_WORDS, RELAY_UNREACHABLE_WORDS, RelayConnectFailure,
};
pub use relay_connect_offer::{RELAY_CONNECTING, connect_relay_offer};
pub use relay_connect_op::RelayConnectOp;
pub use relay_transport::{RelayDeviceTransport, RelayLinkSource};
pub use runtime_backing::{Backing, EMULATED_TARGETS, backing_for, emu_offered_for};
pub use runtime_band::{UiRuntimeBand, speed_word};
pub use shared_link_client_io::{ConversationInbox, SharedLinkClientIo};
pub use sim_create_op::{SimCreateOp, sim_device_name};
pub use sim_record::{
    BLE_ENDPOINT_PREFIX, NewSimRecord, RuntimeKind, SimRecord, ble_endpoint, ble_link_info,
    delete_sim_record, device_id_from_ble_endpoint, emu_endpoint, emu_link_info, mint_sim_identity,
    new_sim_record, read_sim_record, sim_endpoint, sim_link_info, uid_from_emu_endpoint,
    uid_from_sim_endpoint, write_sim_record,
};
pub use sim_transport::{
    SimBacking, SimDeviceTransport, SimLinkSource, SimRuntimeControl, SimSession, SimTier,
};
pub use store_lookups::{StoreLookup, StoreLookups};
pub use take_over_offer::{TAKE_OVER_ASKING, busy_in_the_other_tab, take_over_offer};
pub use take_over_op::TakeOverOp;
pub use take_over_state::{
    ASK_PATIENCE_SECS, NETWORK_OPEN_PATIENCE_SECS, OPEN_PATIENCE_SECS, TAKE_OVER_ANOTHER_TAB,
    TAKE_OVER_NO_ANSWER, TAKE_OVER_NO_WAY, TAKE_OVER_OPENING_WORDS, TAKE_OVER_STILL_IN_USE,
    TakeOverStage, TakeOverTimeout, TakeOvers, UiTakeOver,
};
pub use target_offer::{TargetChoice, TargetGroup, TargetOffer, TargetScope, target_offer};
pub use ui_link_kind::UiLinkKind;
pub use update_build_facts::{StoreLatest, StoreReleases, UpdateBuildFacts};
pub use update_host::UpdateHost;
pub use wifi_address_book::{WIFI_ADDRESSES_STORAGE_KEY, WifiAddress, WifiAddressBook};
pub use wifi_connect_failure::{WIFI_BLOCKED_WORDS, WIFI_BUSY_WORDS, WifiConnectFailure};
pub use wifi_connect_offer::connect_wifi_offer;
pub use wifi_connect_op::WifiConnectOp;
pub use wifi_connects::{UiWifiConnect, WifiConnectTarget, WifiConnects};
