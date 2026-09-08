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
//! | `Link { command }` | [`DeviceEffects`] → the routed [`Link`](lpa_devices::Link) (browser Web Serial on wasm, the fake on the host) |
//! | `StartTimer` | [`DeviceEffects`] → one spawned future per timer on the app's timer factory |
//! | `PersistRecord` / `DeleteRecord` | [`DeviceRoster`] → the kept `places::device_registry`, through the library host's locked catalog |
//! | `RequestUsbGrant` | [`DeviceTransport::request_grant`] → the platform chooser |
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

/// Sims backed by `fw-browser` workers. wasm-only, and only when the studio
/// is built with the provider that owns them.
#[cfg(all(feature = "browser-worker", target_arch = "wasm32"))]
pub mod browser_sim_source;
/// The browser Web Serial transport. wasm-only, and only when the studio is
/// built with the provider that owns the port.
#[cfg(all(feature = "browser-serial-esp32", target_arch = "wasm32"))]
pub mod browser_transport;
pub mod composite_transport;
pub mod device_affordance;
pub mod device_by_base_mac;
pub mod device_card_feed_view;
pub mod device_effects;
pub mod device_feed_op;
pub mod device_firmware_face;
pub mod device_flash;
pub mod device_frame_feed;
pub mod device_frame_snapshot;
pub mod device_identity;
pub mod device_push;
pub mod device_records;
pub mod device_roster;
pub mod device_transport;
pub mod devices_op;
pub mod runtime_band;
pub mod shared_link_client_io;
pub mod sim_record;
pub mod sim_transport;

#[cfg(all(feature = "browser-worker", target_arch = "wasm32"))]
pub use browser_sim_source::BrowserSimLinkSource;
#[cfg(all(feature = "browser-serial-esp32", target_arch = "wasm32"))]
pub use browser_transport::BrowserSerialTransport;
pub use composite_transport::CompositeDeviceTransport;
pub use device_affordance::{
    device_escape_action, device_escape_action_for, device_status_kind, pending_escape_action,
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
pub use device_flash::{
    FirmwareVerb, FlashBoardChoice, FlashOffer, derive_flash_name, firmware_verb, flash_offer,
    flash_offer_for, reflash_choice, taken_device_titles,
};
pub use device_frame_feed::{DEVICE_FEED_PARK_AFTER_FAILURES, DeviceFrameFeed, DeviceFrameFeeds};
pub use device_frame_snapshot::DEVICE_FRAME_SNAPSHOT_INTERVAL_SECS;
pub use device_identity::{
    DeviceIdentityLine, IdentityFirmware as DeviceIdentityFirmware, IdentityRows, device_chip,
    device_identity_line, pending_identity_rows,
};
pub use device_push::{
    DevicePushOp, PushOffer, PushSource, PushSourceChoice, PushSourceGroup,
    first_bundled_example_id, push_offer,
};
pub use device_records::{
    SIM_TRANSPORT, USB_TRANSPORT, auto_record_name, record_from_registry_row,
    registry_row_from_record, transport_label_for_endpoint,
};
pub use device_roster::{
    DeviceRoster, DeviceRosterView, JournalLine, RememberedView, RosterSplit, split_roster,
};
pub use device_transport::{
    DeviceEffectCall, DeviceEffectFacts, DeviceEffectProgress, DeviceTransport,
    DeviceTransportFuture, GrantedLink, LensLineTap, LensTapEvent,
};
pub use devices_op::{DeviceFace, DevicesOp};
pub use runtime_band::UiRuntimeBand;
pub use shared_link_client_io::{ConversationInbox, SharedLinkClientIo};
pub use sim_record::{
    NewSimRecord, SimRecord, delete_sim_record, mint_sim_identity, new_sim_record, read_sim_record,
    sim_endpoint, sim_link_info, uid_from_sim_endpoint, write_sim_record,
};
pub use sim_transport::{
    SimBacking, SimDeviceTransport, SimLinkSource, SimRuntimeControl, SimSession, SimTier,
};
