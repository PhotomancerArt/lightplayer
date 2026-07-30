//! ESP32-S3 chip facts.
//!
//! Chip-specific values live here rather than in `fw-esp32-common`: the seam
//! rule is that shared firmware code never learns chip facts, it receives them.

pub mod constants;
pub mod cycle_counter;
