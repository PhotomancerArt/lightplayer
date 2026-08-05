//! WS281x output over the classic-ESP32 RMT peripheral.
//!
//! Three layers, deliberately separated — the same split fw-esp32s3 uses:
//!
//! * [`v3_rmt`] — the chip. Seven register operations implementing
//!   [`lp_ws281x::RmtHw`], and the only place in this firmware that knows a
//!   classic-ESP32 RMT address.
//! * [`shared_driver`] — the one [`lp_ws281x::Ws281xDriver`] instance, the RMT
//!   interrupt trampoline that feeds it, and the optional telemetry tap. All
//!   sequencing lives in the core crate, tested on the host.
//! * [`esp32v3_rmt_ws281x_driver`] — the `lpc-hardware` seam: endpoints,
//!   leases, open-time pin binding, and the manifest-index → RMT-slot mapping
//!   that two blocks per channel makes necessary.
//! * [`wire_pusher`] — the dual-core overlap deployment: the per-wire
//!   mailboxes the PRO core posts frames into, the APP-core loop that runs
//!   [`lp_ws281x::Pusher`] (admission, slot binding, matrix re-mux, starts),
//!   and the software-interrupt doorbell that wakes it. The single-core
//!   fallback never touches it.
//!
//! A fourth module, [`frame_dump`], exists only under the `frame-dump` feature:
//! it is the serial transcript of what the channels transmitted, and it hangs
//! off the app driver's write path. Nothing else depends on it, and it is
//! independent of `ws281x_telemetry` — that one taps the same write path to
//! answer a question about refill budget rather than about pixel values.

pub mod shared_driver;
pub mod v3_rmt;
pub mod wire_pusher;

mod esp32v3_rmt_ws281x_driver;

#[cfg(feature = "frame-dump")]
pub mod frame_dump;

#[cfg(feature = "ws281x_telemetry")]
pub mod refill_floor_probe;

pub use esp32v3_rmt_ws281x_driver::Esp32V3RmtWs281xDriver;
