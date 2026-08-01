//! Classic ESP32 (LX6) chip facts.
//!
//! Chip-specific values live here rather than in `fw-esp32-common`: the seam
//! rule is that shared firmware code never learns chip facts, it receives
//! them (ADR `2026-07-29-per-chip-fw-toolchains`).
//!
//! There is no `usb_connection` sibling to fw-esp32s3's: this chip has no
//! USB-Serial-JTAG peripheral, so there is no SOF signal to poll and no
//! enumeration state to track. See `crate::serial` for what replaces it.

// The app entrypoint's sole source of the peripheral singleton. See the module
// doc for the hazard that makes it the *only* one.
pub mod init;
