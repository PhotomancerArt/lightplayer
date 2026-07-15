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
//! Runtime neutrality: this module never spawns tasks and never sleeps on a
//! concrete executor. All timing comes through the injected [`DeviceTimers`]
//! factory, and readiness is driven on demand from the session's own async
//! methods (`wait_ready`, or the channel's first use).

mod device_client_io;
mod device_event;
mod device_mode;
mod device_readiness;
mod device_session;
mod device_snapshot;
mod device_state;
mod device_timers;

#[cfg(all(test, feature = "fake-device"))]
mod tests;

pub use device_event::{DeviceEvent, DeviceEventSink};
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
