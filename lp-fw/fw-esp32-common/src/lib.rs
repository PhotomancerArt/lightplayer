//! Chip-generic firmware layer shared by per-SOC ESP32 firmware crates.
//!
//! Everything here is buildable under both the pinned workspace nightly and the
//! Espressif Xtensa fork: no esp-* HAL dependencies, no `unwinding`, no panic
//! strategy. Chip facts are injected by the bin crate — a memory-stats function
//! for the server loop, serial write channels for the transport, and a fallback
//! manifest for the loader. See `docs/adr/2026-07-29-per-chip-fw-toolchains.md`
//! for the seam rules.

#![no_std]
// The seam functions (`lp_seam::seam_fn!`) expand an `asm!` hint here, and
// Xtensa `asm!` is still unstable upstream.
#![cfg_attr(target_arch = "xtensa", feature(asm_experimental_arch))]
#![cfg_attr(
    target_arch = "xtensa",
    allow(
        unstable_features,
        reason = "asm_experimental_arch is required for the Xtensa seam functions' hint"
    )
)]

extern crate alloc;

pub mod flash_qio;
pub mod flash_read_mode;
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
// RISC-V and Xtensa (since S3 radio M5); host unit tests build it too, over
// the `lp-seam` dev-dependency.
#[cfg(any(target_arch = "riscv32", target_arch = "xtensa", test))]
pub mod seams;
pub mod serial;
pub mod time;

#[cfg(feature = "fs-tree")]
pub mod aligned_nor_flash;
#[cfg(feature = "server")]
pub mod boot;
#[cfg(feature = "server")]
pub mod chip_identity;
// Every server build: an `fs-tree` board always guards, a littlefs board
// that holds a tree store guards too (ADR 2026-10-10, amended 2026-10-16).
#[cfg(feature = "server")]
pub mod fs_tree_core_guard;
#[cfg(feature = "server")]
pub mod hardware;
#[cfg(feature = "server")]
pub mod link_upkeep;
#[cfg(feature = "server")]
pub mod lp_fs;
pub mod radio_link;
#[cfg(feature = "server")]
pub mod server_loop;
#[cfg(feature = "split-core")]
pub mod split_core;
#[cfg(feature = "fs-tree")]
pub mod tree_fs;
#[cfg(feature = "uart-link")]
pub mod uart_link;
pub mod update_send;
#[cfg(feature = "usb-link")]
pub mod usb_link;
