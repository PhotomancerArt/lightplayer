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

/// The browser Web Serial transport. wasm-only, and only when the studio is
/// built with the provider that owns the port.
#[cfg(all(feature = "browser-serial-esp32", target_arch = "wasm32"))]
pub mod browser_transport;
pub mod device_affordance;
pub mod device_effects;
pub mod device_firmware_face;
pub mod device_flash;
pub mod device_identity;
pub mod device_push;
pub mod device_records;
pub mod device_roster;
pub mod device_transport;
pub mod devices_op;

#[cfg(all(feature = "browser-serial-esp32", target_arch = "wasm32"))]
pub use browser_transport::BrowserSerialTransport;
pub use device_affordance::{device_escape_action, device_status_kind, pending_escape_action};
pub use device_effects::{
    CompletedPush, DeviceEffects, DeviceTaskFuture, DeviceTimerFuture, PendingWrites, PushPayload,
    StagedPush,
};
pub use device_firmware_face::{
    device_firmware_line, firmware_face_preview_sentence, pending_firmware_line,
};
pub use device_flash::{
    FirmwareVerb, FlashBoardChoice, FlashOffer, derive_flash_name, firmware_verb, flash_offer,
    flash_offer_for, reflash_choice, taken_device_titles,
};
pub use device_identity::{DeviceIdentityLine, device_chip, device_identity_line};
pub use device_push::{
    DevicePushOp, PushOffer, PushSource, PushSourceChoice, PushSourceGroup,
    first_bundled_example_id, push_offer,
};
pub use device_records::{auto_record_name, record_from_registry_row, registry_row_from_record};
pub use device_roster::{
    DeviceRoster, DeviceRosterView, JournalLine, RememberedView, RosterSplit, split_roster,
};
pub use device_transport::{
    DeviceEffectCall, DeviceEffectFacts, DeviceEffectProgress, DeviceTransport,
    DeviceTransportFuture, GrantedLink, LensLineTap, LensTapEvent,
};
pub use devices_op::DevicesOp;
