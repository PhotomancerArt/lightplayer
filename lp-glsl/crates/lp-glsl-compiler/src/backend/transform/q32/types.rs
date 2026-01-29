//! Fixed-point format types and conversion utilities.

use cranelift_codegen::ir::Type;

/// Fixed-point format selection
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FixedPointFormat {
    /// 16.16 format: 16 integer bits, 16 fractional bits (uses I32)
    /// Range: -32768.0 to +32767.9999847412109375
    /// Precision: 1/65536 (approximately 0.00001526)
    Fixed16x16,
    /// 32.32 format: 32 integer bits, 32 fractional bits (uses I64)
    /// Note: Not yet fully implemented
    #[allow(dead_code, reason = "Reserved for future q32x32 implementation")]
    Q32x32,
}

impl FixedPointFormat {
    /// Get the Cranelift type for this fixed-point format
    pub fn cranelift_type(&self) -> Type {
        match self {
            FixedPointFormat::Fixed16x16 => cranelift_codegen::ir::types::I32,
            FixedPointFormat::Q32x32 => cranelift_codegen::ir::types::I64,
        }
    }

    /// Get the shift amount for this format
    pub fn shift_amount(&self) -> i64 {
        match self {
            FixedPointFormat::Fixed16x16 => 16,
            FixedPointFormat::Q32x32 => 32,
        }
    }
}

/// Convert a float32 value to fixed16x16 representation.
///
/// Fixed16x16 format uses 16 integer bits and 16 fractional bits.
/// Range: -32768.0 to +32767.9999847412109375
/// Precision: 1/65536 (approximately 0.00001526)
pub fn float_to_fixed16x16(f: f32) -> i32 {
    // Clamp to representable range
    let clamped = f.clamp(-32768.0, 32767.9999847412109375);
    // Convert to fixed-point (round to nearest)
    let scaled = clamped * crate::frontend::codegen::constants::FIXED16X16_SCALE;
    let rounded = if scaled >= 0.0 {
        (scaled + 0.5) as i32
    } else {
        (scaled - 0.5) as i32
    };
    rounded
}

/// Convert fixed16x16 back to float32 (for debugging/constants).
#[allow(
    dead_code,
    reason = "Utility function for debugging and constant conversion"
)]
pub fn fixed16x16_to_float(fixed: i32) -> f32 {
    fixed as f32 / crate::frontend::codegen::constants::FIXED16X16_SCALE
}

/// Convert a float32 value to q32x32 representation.
///
/// Q32x32 format uses 32 integer bits and 32 fractional bits.
/// Range: -2147483648.0 to +2147483647.9999999998
/// Precision: 1/4294967296 (approximately 0.00000000023)
#[allow(dead_code, reason = "Reserved for future use")]
pub fn float_to_q32x32(f: f32) -> i64 {
    // Convert to f64 for more precision in intermediate calculations
    let f64_val = f as f64;
    // Clamp to representable range
    let clamped = f64_val.clamp(-2147483648.0, 2147483647.9999999998);
    // Convert to fixed-point (round to nearest)
    let scaled = clamped * 4294967296.0;
    let rounded = if scaled >= 0.0 {
        (scaled + 0.5) as i64
    } else {
        (scaled - 0.5) as i64
    };
    rounded
}

/// Convert q32x32 back to float32 (for debugging/constants).
#[allow(
    dead_code,
    reason = "Utility function for debugging and constant conversion"
)]
pub fn q32x32_to_float(fixed: i64) -> f32 {
    (fixed as f64 / 4294967296.0) as f32
}
