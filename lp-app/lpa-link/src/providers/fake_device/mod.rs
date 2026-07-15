//! Scriptable fake ESP32 device (feature `fake-device`, host only).
//!
//! A byte-level fake: a [`FakeEsp32Device`] implements the
//! [`DeviceByteStream`](crate::stream::DeviceByteStream) seam and scripts
//! what a real board does on the wire — ROM boot output for blank flash /
//! download mode / foreign firmware, and for the `LightPlayer` state a REAL
//! host `LpServer` over `LpFsMemory` behind REAL `M!` framing (reusing
//! `fw-host`'s server-over-memory machinery), including the M2 boot line and
//! the unsolicited wire hello.
//!
//! Every hardware bug so far (pull-before-readiness ordering, fresh-device
//! missing storage dir) lived BELOW the record level: framing, boot-output
//! classification, timing. Injecting at the byte stream makes the real `M!`
//! parser, the real readiness classifier, and the real orchestration run in
//! tests. The `FakeProvider` exposes these devices through the real link
//! provider path (see `providers::fake`).
//!
//! Reset-signal handling mirrors hardware: the hard-reset DTR/RTS dance
//! replays the current state's boot; the usb-jtag-download dance drops the
//! device into `RomDownloadMode`. Failure injection
//! ([`FakeFailurePlan`]) composes on the stream: latency, stall-after-N,
//! disconnect, garble/drop, mid-frame cut, log-flood interleave.

pub mod failure_injection;
pub mod fake_device_core;
pub mod fake_device_script;
pub mod fake_device_stream;

pub use failure_injection::FakeFailurePlan;
pub use fake_device_core::FakeEsp32Device;
pub use fake_device_script::{
    FAKE_DEVICE_PROJECT_DIR, FAKE_IMAGE_IDENTITY, FakeBootState, FakeDeviceIdentity,
    FakeDeviceScript, FakeLightPlayerState, fake_provenance,
};
pub use fake_device_stream::FakeDeviceByteStream;

#[cfg(test)]
mod tests;
