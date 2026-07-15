//! Hardware device session above the connector layer.
//!
//! [`DeviceSession`] owns one hardware link end-to-end: the connector that
//! opened it, the live link session/connection, and the app-protocol channel
//! consumers speak through. It exposes an observable [`DeviceState`] machine
//! with **hello-first readiness**: the session is ready ONLY when the
//! unsolicited wire [`lpc_wire::ServerHello`] arrives with a matching
//! [`lpc_wire::WIRE_PROTO_VERSION`]. Boot-line classification
//! ([`BootLineClassifier`]) is diagnosis-only — it explains why a device is
//! not ready (blank flash, ROM bootloader, foreign firmware, silence) and
//! never grants readiness.
//!
//! Sim runtimes (browser worker) bypass this module entirely: they have no
//! boot, no hello race, and no management plane. `DeviceSession` is
//! hardware-only by design.
//!
//! Management ([`DeviceSession::manage`]) owns the flash/erase/reset cycle
//! with mode-exclusive wire access: release the link (the old transport's
//! serial thread ends), run the connector operation, then **reconnect =
//! rebuild** — a brand-new provider session + transport with readiness
//! re-run from `Booting`. The same rebuild is [`DeviceSession::reconnect`],
//! the one way out of the otherwise-sticky terminal states (`Gone`,
//! `Incompatible`, diagnosis states).
//!
//! Runtime neutrality: this module never spawns tasks and never sleeps on a
//! concrete executor. All timing comes through the injected [`DeviceTimers`]
//! factory, and readiness is driven on demand from the session's own async
//! methods (`wait_ready`, or the channel's first use).

mod device_client_io;
mod device_event;
mod device_manage;
mod device_mode;
mod device_readiness;
mod device_session;
mod device_snapshot;
mod device_state;
mod device_timers;
mod device_wire;

#[cfg(all(test, feature = "fake-device"))]
mod tests;

pub use device_event::{DeviceEvent, DeviceEventSink, DeviceLineOrigin};
pub use device_manage::DeviceManageOutcome;
pub use device_mode::{DeviceMode, DeviceModeGuard};
pub use device_readiness::{
    BootDiagnosis, BootLineClassifier, NO_FIRMWARE_DETECTED_PREFIX, NoFirmwareReason,
    is_no_firmware_detected_message,
};
pub use device_session::DeviceSession;
pub use device_snapshot::DeviceSnapshot;
pub use device_state::{DeviceState, IncompatibleReason};
pub use device_timers::{
    DEFAULT_CONNECT_DEADLINE, DEFAULT_READY_DEADLINE, DEFAULT_REQUEST_IDLE_DEADLINE,
    DeviceDeadlines, DeviceTimerFuture, DeviceTimers, READINESS_POLL_INTERVAL,
};
