//! Constants used in code generation
//!
//! This module defines constants used throughout the code generation process,
//! such as type sizes, alignment values, and fixed-point format scales.

/// Size of an f32 in bytes
pub const F32_SIZE_BYTES: usize = 4;

/// Alignment shift for f32 values (2^2 = 4 bytes)
pub const F32_ALIGN_SHIFT: u8 = 2;

/// Scale factor for fixed16x16 format (2^16 = 65536)
pub const FIXED16X16_SCALE: f32 = 65536.0;

/// Precision of fixed16x16 format (1/65536)
pub const FIXED16X16_PRECISION: f32 = 1.0 / 65536.0;
