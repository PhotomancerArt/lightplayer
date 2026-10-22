//! The C6's facts for `lp-net` (`fw_esp32_radio::net::net_thread::NetChip`):
//! the thread's stack and priority, and the chip code the network calls.

use fw_esp32_radio::net::net_thread::NetChip;

/// `lp-net`'s stack, bytes: 8 KB (plan P03; the secure handshake needs
/// ~3.9 KB on top of the tasks' own frames, and P08 set the figure from the
/// `[netstack]` high-water).
pub const STACK_BYTES: usize = 8 * 1024;
/// `lp-net`'s priority: the link thread's (`io_thread`).
pub const PRIORITY: u32 = 1;

/// What `net_thread::start` is given on this chip (by reference: a
/// `static`, in flash).
pub static NET_CHIP: NetChip = NetChip {
    stack_bytes: STACK_BYTES,
    priority: PRIORITY,
    on_thread_start,
    first_light_decided: crate::first_light::decided,
    on_join_attempt,
    bind_seam_wake: crate::seams::seam_wake_handler::bind,
};

/// First thing on `lp-net`: paint its stack (`net_thread_stack_diag`).
fn on_thread_start() {
    #[cfg(feature = "net_thread_stack_diag")]
    super::net_thread_stack_diag::paint();
}

/// Before each join: track the heap from here (`heap_map_diag`).
fn on_join_attempt() {
    #[cfg(feature = "heap_map_diag")]
    crate::heap_map::arm_tracking("station join");
}

/// After the heartbeat's joined `[wifi]` line (`radio_dma_diag`); passed to
/// `net_heartbeat::log_line`.
pub fn on_heartbeat_joined() {
    #[cfg(feature = "radio_dma_diag")]
    crate::radio_dma_diag::log("joined");
}
