//! Chip-generic firmware layer shared by per-SOC ESP32 firmware crates.
//!
//! Everything here is buildable under both the pinned workspace nightly and the
//! Espressif Xtensa fork: no esp-* HAL dependencies, no `unwinding`, no panic
//! strategy. Chip facts are injected by the bin crate — a memory-stats function
//! for the server loop, serial write channels for the transport, and a fallback
//! manifest for the loader. See `docs/adr/2026-07-29-per-chip-fw-toolchains.md`
//! for the seam rules.

#![no_std]

extern crate alloc;

#[cfg(feature = "frame-pace-diag")]
pub mod frame_pace_diag;
pub mod frame_time_stats;
pub mod jit_fns;
pub mod largest_block;
#[cfg(any(feature = "usb-link", feature = "uart-link"))]
pub mod link_lock;
#[cfg(any(feature = "usb-link", feature = "uart-link"))]
pub mod log_ring_logger;
pub mod logger;
#[cfg(feature = "wifi")]
pub mod net;
pub mod output;
// Host unit tests build it too, over the `lp-seam` dev-dependency.
#[cfg(any(target_arch = "riscv32", test))]
pub mod seams;
pub mod serial;
pub mod time;

#[cfg(feature = "server")]
pub mod boot;
#[cfg(feature = "server")]
pub mod chip_identity;
#[cfg(feature = "server")]
pub mod hardware;
#[cfg(feature = "server")]
pub mod link_upkeep;
#[cfg(feature = "server")]
pub mod lp_fs;
pub mod radio_link;
#[cfg(feature = "server")]
pub mod server_loop;
#[cfg(feature = "uart-link")]
pub mod uart_link;
pub mod update_send;
#[cfg(feature = "usb-link")]
pub mod usb_link;
