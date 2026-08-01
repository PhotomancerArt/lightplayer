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

pub mod jit_fns;
pub mod logger;
pub mod output;
pub mod serial;
pub mod time;

#[cfg(feature = "server")]
pub mod boot;
#[cfg(feature = "server")]
pub mod hardware;
#[cfg(feature = "server")]
pub mod lp_fs;
#[cfg(feature = "server")]
pub mod server_loop;
#[cfg(feature = "server")]
pub mod transport;
