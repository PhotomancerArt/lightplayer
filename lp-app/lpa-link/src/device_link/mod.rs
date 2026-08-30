//! `lpa-devices`' transport contract, implemented over this crate's
//! transports (feature `device-link`).
//!
//! The dependency runs THIS way on purpose (vision R2, invariant "dependency
//! inversion"): the device model defines `Link`, `LinkEvent`, `LinkCommand`
//! and `ResetKind`, and `lpa-link` adapts to them. The model never calls a
//! transport, and — just as important — no transport classifies a device. The
//! hello gate, the boot-line diagnosis and the foreign-firmware detection all
//! live in the device fold, which is what makes verdicts non-sticky. An
//! adapter here that "helpfully" decided a board was blank would put the
//! fifth state machine back.
//!
//! ```text
//!   Roster ──Command::Link──► Link::submit    ─┐
//!                                             │  lpa-link owns the IO
//!   Roster ◄──Event::Link──── Link::poll_event ┘
//! ```
//!
//! | module | what it adapts |
//! |---|---|
//! | [`wire`] | `lpc_wire` frames ⇄ the model's minimal mirror (the ONE meeting point) |
//! | [`demux`] | whole serial lines → `LinkEvent`s (the `M!` demux) |
//! | `byte_stream` | the sync `DeviceByteStream` seam → `Link` (host) |
//! | `fake` | the scripted `FakeEsp32Device` → `Link` (host tests) |
//! | `browser_serial` | the Web Serial provider → `Link` (wasm) |
//!
//! What is NOT here: the effects layer. Pumping `poll_event` into
//! `Event::Link`, running timers, persisting records and revoking grants are
//! the app's job (M3's studio-core slice) — this module only makes the
//! transports speak the contract.

pub mod demux;
pub mod wire;

#[cfg(any(
    feature = "host-process",
    feature = "host-serial-esp32",
    feature = "fake-device"
))]
pub mod byte_stream;

#[cfg(feature = "fake-device")]
pub mod fake;

#[cfg(all(feature = "browser-serial-esp32", target_arch = "wasm32"))]
pub mod browser_serial;

#[cfg(all(test, feature = "fake-device"))]
mod tests;
