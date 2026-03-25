//! Q16.16 fixed-point helpers for Q32 emission.

use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_codegen::ir::{InstBuilder, Value, types};
use cranelift_frontend::FunctionBuilder;

const Q32_SHIFT: i64 = 16;
const Q32_SCALE: f64 = 65536.0;
const Q32_MAX: i64 = 0x7FFF_FFFF;
const Q32_MIN: i64 = i32::MIN as i64;
const Q32_FRAC: i32 = (1 << Q32_SHIFT) - 1;

/// Encode an f32 constant as a Q16.16 fixed-point i32.
pub(crate) fn q32_encode(value: f32) -> i32 {
    q32_encode_f64(f64::from(value))
}

/// Encode `f64` as Q16.16 (Level-1 call interchange).
pub(crate) fn q32_encode_f64(value: f64) -> i32 {
    let scaled = (value * Q32_SCALE).round();
    if scaled > Q32_MAX as f64 {
        Q32_MAX as i32
    } else if scaled < Q32_MIN as f64 {
        i32::MIN
    } else {
        scaled as i32
    }
}

/// Decode Q16.16 fixed-point to `f64`.
pub(crate) fn q32_to_f64(raw: i32) -> f64 {
    f64::from(raw) / Q32_SCALE
}

pub(crate) fn emit_fneg(builder: &mut FunctionBuilder, v: Value) -> Value {
    builder.ins().ineg(v)
}

pub(crate) fn emit_fabs(builder: &mut FunctionBuilder, v: Value) -> Value {
    let zero = builder.ins().iconst(types::I32, 0);
    let neg = builder.ins().ineg(v);
    let is_neg = builder.ins().icmp(IntCC::SignedLessThan, v, zero);
    builder.ins().select(is_neg, neg, v)
}

pub(crate) fn emit_fmin(builder: &mut FunctionBuilder, a: Value, b: Value) -> Value {
    let cmp = builder.ins().icmp(IntCC::SignedLessThanOrEqual, a, b);
    builder.ins().select(cmp, a, b)
}

pub(crate) fn emit_fmax(builder: &mut FunctionBuilder, a: Value, b: Value) -> Value {
    let cmp = builder.ins().icmp(IntCC::SignedGreaterThanOrEqual, a, b);
    builder.ins().select(cmp, a, b)
}

pub(crate) fn emit_ffloor(builder: &mut FunctionBuilder, v: Value) -> Value {
    let int_mask = builder.ins().iconst(types::I32, i64::from(!Q32_FRAC));
    let truncated = builder.ins().band(v, int_mask);
    let frac_mask = builder.ins().iconst(types::I32, i64::from(Q32_FRAC));
    let frac = builder.ins().band(v, frac_mask);
    let zero = builder.ins().iconst(types::I32, 0);
    let has_frac = builder.ins().icmp(IntCC::NotEqual, frac, zero);
    let is_neg = builder.ins().icmp(IntCC::SignedLessThan, v, zero);
    let needs_adjust = builder.ins().band(has_frac, is_neg);
    let one = builder.ins().iconst(types::I32, 1 << Q32_SHIFT);
    let adjusted = builder.ins().isub(truncated, one);
    builder.ins().select(needs_adjust, adjusted, truncated)
}

pub(crate) fn emit_fceil(builder: &mut FunctionBuilder, v: Value) -> Value {
    let int_mask = builder.ins().iconst(types::I32, i64::from(!Q32_FRAC));
    let truncated = builder.ins().band(v, int_mask);
    let frac_mask = builder.ins().iconst(types::I32, i64::from(Q32_FRAC));
    let frac = builder.ins().band(v, frac_mask);
    let zero = builder.ins().iconst(types::I32, 0);
    let has_frac = builder.ins().icmp(IntCC::NotEqual, frac, zero);
    let is_pos = builder.ins().icmp(IntCC::SignedGreaterThanOrEqual, v, zero);
    let needs_adjust = builder.ins().band(has_frac, is_pos);
    let one = builder.ins().iconst(types::I32, 1 << Q32_SHIFT);
    let adjusted = builder.ins().iadd(truncated, one);
    builder.ins().select(needs_adjust, adjusted, truncated)
}

pub(crate) fn emit_ftrunc(builder: &mut FunctionBuilder, v: Value) -> Value {
    let int_mask = builder.ins().iconst(types::I32, i64::from(!Q32_FRAC));
    builder.ins().band(v, int_mask)
}

/// Q16.16 → signed integer (truncate toward zero, like C cast)
pub(crate) fn emit_to_sint(builder: &mut FunctionBuilder, v: Value) -> Value {
    let shift = builder.ins().iconst(types::I32, Q32_SHIFT);
    let zero = builder.ins().iconst(types::I32, 0);
    let bias_mask = builder.ins().iconst(types::I32, i64::from(Q32_FRAC));
    let is_neg = builder.ins().icmp(IntCC::SignedLessThan, v, zero);
    let biased = builder.ins().iadd(v, bias_mask);
    let biased_value = builder.ins().select(is_neg, biased, v);
    builder.ins().sshr(biased_value, shift)
}

/// Signed integer → Q16.16 (clamp to representable range, then shift)
pub(crate) fn emit_from_sint(builder: &mut FunctionBuilder, v: Value) -> Value {
    let shift = builder.ins().iconst(types::I32, Q32_SHIFT);
    let max_int = builder.ins().iconst(types::I32, 32767);
    let min_int = builder.ins().iconst(types::I32, -32768);
    let clamped = builder.ins().smin(v, max_int);
    let clamped = builder.ins().smax(clamped, min_int);
    builder.ins().ishl(clamped, shift)
}

/// Q16.16 → unsigned integer (clamp negatives to 0)
pub(crate) fn emit_to_uint(builder: &mut FunctionBuilder, v: Value) -> Value {
    let trunc = emit_to_sint(builder, v);
    let zero = builder.ins().iconst(types::I32, 0);
    let is_neg = builder.ins().icmp(IntCC::SignedLessThan, trunc, zero);
    builder.ins().select(is_neg, zero, trunc)
}

/// Unsigned integer → Q16.16 (shift left, no sign extension needed)
pub(crate) fn emit_from_uint(builder: &mut FunctionBuilder, v: Value) -> Value {
    let shift = builder.ins().iconst(types::I32, Q32_SHIFT);
    builder.ins().ishl(v, shift)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_basics() {
        assert_eq!(q32_encode(0.0), 0);
        assert_eq!(q32_encode(1.0), 65536);
        assert_eq!(q32_encode(-1.0), -65536);
        assert_eq!(q32_encode(1.5), 98304);
        assert_eq!(q32_encode(0.5), 32768);
    }

    #[test]
    fn encode_saturation() {
        assert_eq!(q32_encode(40000.0), 0x7FFF_FFFF);
        assert_eq!(q32_encode(-40000.0), i32::MIN);
    }
}
