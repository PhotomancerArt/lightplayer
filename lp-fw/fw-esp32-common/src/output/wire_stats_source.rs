//! The seam through which a chip crate hands per-wire output telemetry to
//! the heartbeat.
//!
//! The per-wire counters live in chip-specific statics (fw-esp32v3's
//! `wire_pusher::MAILBOXES`); the heartbeat is built here in the common
//! server loop. Rather than a cross-crate dependency, the chip installs a
//! collector function at init — the same posture as `lp_recovery`'s
//! installed global — and a chip with no per-wire attribution (the C6, the
//! S3, a single-core fallback boot) simply never installs one, which is
//! what makes the heartbeat field honestly absent there.

use core::sync::atomic::AtomicUsize;
use core::sync::atomic::Ordering::{Acquire, Release};

extern crate alloc;
use alloc::vec::Vec;

use lpc_wire::server::OutputWireStatus;

/// The installed collector, as a function address (0 = none). A plain fn
/// pointer, stored once at init before the server loop starts.
static SOURCE: AtomicUsize = AtomicUsize::new(0);

type Collector = fn() -> Vec<OutputWireStatus>;

/// Install the chip's collector. Call once at init, before the server loop
/// runs; later calls replace the source (harmless, unused).
pub fn install(collector: Collector) {
    SOURCE.store(collector as usize, Release);
}

/// Collect the current per-wire status, if a collector is installed and
/// reports any wires. `None` keeps the heartbeat field absent.
pub fn current() -> Option<Vec<OutputWireStatus>> {
    let raw = SOURCE.load(Acquire);
    if raw == 0 {
        return None;
    }
    // SAFETY: the only non-zero values ever stored are `Collector` fn
    // addresses from `install`, and fn pointers are 'static.
    let collector: Collector = unsafe { core::mem::transmute::<usize, Collector>(raw) };
    let stats = collector();
    if stats.is_empty() { None } else { Some(stats) }
}
