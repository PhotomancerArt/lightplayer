//! The S3's facts for `lp-net` (`fw_esp32_radio::net::net_thread::NetChip`):
//! the thread's stack, priority and core, and the chip code the network
//! calls.

use core::sync::atomic::{AtomicBool, Ordering};

use fw_esp32_radio::net::net_thread::NetChip;

/// `lp-net`'s stack, bytes: the C6's 8 KB (its secure handshake needs
/// ~3.9 KB on top of the tasks' own frames) plus half again, because the
/// Xtensa windowed ABI's frames are larger than RV32's — the link thread took
/// the same margin (`io_thread`: 4 KB against the C6's 3 KB). Unmeasured on
/// this chip; the C6's `net_thread_stack_diag` is how it would be.
pub const STACK_BYTES: usize = 12 * 1024;
/// `lp-net`'s priority: the link thread's.
pub const PRIORITY: u32 = 1;
/// The core `lp-net` is pinned to: core 0, as the link thread is
/// (`io_thread`: an unpinned thread in this `multi_core` esp-rtos build can
/// try to schedule core 1, which this firmware never starts).
pub const CORE: u32 = 0;
/// The radio's TX power cap: 15 dBm (60 quarter dBm). At the blob's 20 dBm
/// FS3 (a XIAO ESP32-S3 Plus) never authenticated, 0 of 26 attempts, while
/// it heard the access point at −15…−44 dBm; capped at 15 dBm it joined 10
/// of 10 on the first attempt
/// (docs/defects/2026-10-22-the-s3-wifi-image-never-authenticates.md).
pub const MAX_TX_POWER_QDBM: i8 = 60;

/// What `net_thread::start` is given on this chip.
pub static NET_CHIP: NetChip = NetChip {
    stack_bytes: STACK_BYTES,
    priority: PRIORITY,
    pin_to_core: Some(CORE),
    on_thread_start: nothing,
    first_light_decided,
    on_join_attempt: nothing,
    max_tx_power_qdbm: Some(MAX_TX_POWER_QDBM),
    bind_seam_wake: crate::seams::seam_wake_handler::bind,
};

/// The server loop has served its first frame (set by the engine's frame
/// hook, [`note_frame_served`]).
static FIRST_FRAME: AtomicBool = AtomicBool::new(false);

/// The engine's frame hook says a frame was served. One relaxed store a
/// frame.
pub fn note_frame_served() {
    FIRST_FRAME.store(true, Ordering::Relaxed);
}

/// This boot's first light, as far as this chip knows it: the first frame
/// served. The C6 decides it on a lit frame (`first_light`); the S3 has no
/// such ledger yet, and the station's own 2 s cap covers an engine-less core.
/// The boot's first join waits for it, so the WPA2 key derivation (in
/// software, inside esp-radio's Wi-Fi task, above every thread of ours) does
/// not land on the project load or the shader compile.
fn first_light_decided() -> bool {
    FIRST_FRAME.load(Ordering::Relaxed)
}

/// The hooks this chip has no diagnostic behind.
fn nothing() {}

/// After the heartbeat's joined `[wifi]` line: nothing on this chip (the
/// C6's `radio_dma_diag` log); passed to `net_heartbeat::log_line`.
pub fn on_heartbeat_joined() {}
