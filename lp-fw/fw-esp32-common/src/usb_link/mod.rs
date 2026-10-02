//! The USB-Serial-JTAG host link on lp-link (plan `lp-link-usb-cutover`, P2;
//! `docs/adr/2026-09-27-lp-link-one-comms-layer.md`).
//!
//! lp-link runs exactly where USB bytes enter and leave the board (D1). One
//! [`Link`](lp_link::Link) per boot, shared by two parties on the one thread
//! executor:
//!
//! - the **link task** ([`usb_link_task::run_usb_link`]), which owns the
//!   USB-Serial-JTAG halves: it feeds received bytes to the link, writes the
//!   frames the link produces (through the IN-endpoint gate), moves log
//!   records from the ring onto the log channel, and sleeps until a timer,
//!   input or a send doorbell;
//! - the **server transport** ([`usb_link_transport::UsbLinkTransport`]),
//!   which takes whole wire messages off the proto channel and queues replies
//!   onto it ([`crate::serial::server_payload`] for the bytes).
//!
//! The chip crate supplies the register facts ([`usb_link_task::UsbLinkChip`])
//! and spawns the task; nothing here names esp-hal. The BLE links run lp-link
//! too (`crate::radio_link`); the classic's UART keeps its `M!` lines (D3).

pub mod usb_link_counters;
pub mod usb_link_shared;
pub mod usb_link_task;
#[cfg(feature = "server")]
pub mod usb_link_transport;

pub use usb_link_shared::UsbLinkShared;
pub use usb_link_task::{UsbLinkChip, run_usb_link, when_drained};
#[cfg(feature = "server")]
pub use usb_link_transport::UsbLinkTransport;

/// OTA split-link spike: where an update-channel message goes while the
/// engine's transport owns the link (the core installs it before entering
/// the engine). A plain fn pointer in an atomic: set once, read per message.
static UPDATE_HOOK: core::sync::atomic::AtomicUsize = core::sync::atomic::AtomicUsize::new(0);

pub fn set_update_hook(hook: fn(&[u8])) {
    UPDATE_HOOK.store(hook as usize, core::sync::atomic::Ordering::Release);
}

pub(crate) fn on_update_message(data: &[u8]) {
    let raw = UPDATE_HOOK.load(core::sync::atomic::Ordering::Acquire);
    if raw != 0 {
        // SAFETY: only `set_update_hook` stores here, and it stores a `fn(&[u8])`.
        let hook: fn(&[u8]) = unsafe { core::mem::transmute(raw) };
        hook(data);
    }
}
