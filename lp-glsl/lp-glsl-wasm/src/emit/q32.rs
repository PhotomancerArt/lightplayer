//! Q16.16 fixed-point helpers for WASM emission.

use wasm_encoder::{BlockType, InstructionSink, ValType};

const Q16_16_SCALE: f32 = 65536.0;
pub(crate) const Q32_MIN: i64 = i32::MIN as i64;
pub(crate) const Q32_MAX: i64 = i32::MAX as i64;

const MAX_FIXED: i32 = 0x7FFF_FFFF; // Maximum representable fixed-point value
const MIN_FIXED: i32 = i32::MIN; // Minimum representable fixed-point value

/// Convert float to Q16.16 `i32`, clamping to a representable range.
pub(crate) fn f32_to_q16_16(v: f32) -> i32 {
    let clamped = v.clamp(-32768.0, 32767.99998);
    let scaled = (clamped as f64) * f64::from(Q16_16_SCALE);
    if scaled >= f64::from(i32::MAX) {
        return i32::MAX;
    }
    if scaled <= f64::from(i32::MIN) {
        return i32::MIN;
    }
    scaled as i32
}

/// Stack: `i64` value to clamp → leaves one `i32` Q16.16 on the stack.
pub(crate) fn emit_q32_sat_from_i64(sink: &mut InstructionSink<'_>, scratch: u32) {
    let t = BlockType::Result(ValType::I32);
    sink.local_tee(scratch).i64_const(Q32_MIN).i64_lt_s().if_(t);
    sink.i32_const(i32::MIN);
    sink.else_();
    sink.local_get(scratch).i64_const(Q32_MAX).i64_gt_s().if_(t);
    sink.i32_const(i32::MAX);
    sink.else_();
    sink.local_get(scratch).i32_wrap_i64();
    sink.end();
    sink.end();
}

pub(crate) fn emit_q32_fadd(
    sink: &mut InstructionSink<'_>,
    lhs: u32,
    rhs: u32,
    dst: u32,
    scratch: u32,
) {
    sink.local_get(lhs)
        .i64_extend_i32_s()
        .local_get(rhs)
        .i64_extend_i32_s()
        .i64_add();
    emit_q32_sat_from_i64(sink, scratch);
    sink.local_set(dst);
}

pub(crate) fn emit_q32_fsub(
    sink: &mut InstructionSink<'_>,
    lhs: u32,
    rhs: u32,
    dst: u32,
    scratch: u32,
) {
    sink.local_get(lhs)
        .i64_extend_i32_s()
        .local_get(rhs)
        .i64_extend_i32_s()
        .i64_sub();
    emit_q32_sat_from_i64(sink, scratch);
    sink.local_set(dst);
}

pub(crate) fn emit_q32_fmul(
    sink: &mut InstructionSink<'_>,
    lhs: u32,
    rhs: u32,
    dst: u32,
    scratch: u32,
) {
    sink.local_get(lhs)
        .i64_extend_i32_s()
        .local_get(rhs)
        .i64_extend_i32_s()
        .i64_mul()
        .i64_const(16)
        .i64_shr_s();
    emit_q32_sat_from_i64(sink, scratch);
    sink.local_set(dst);
}

pub(crate) fn emit_q32_fdiv(sink: &mut InstructionSink<'_>, lhs: u32, rhs: u32, dst: u32) {
    let t = BlockType::Result(ValType::I32);

    // Check if divisor is zero
    sink.local_get(rhs).i32_const(0).i32_eq().if_(t);
    // Divisor is zero - handle saturation based on dividend sign
    sink.local_get(lhs).i32_const(0).i32_eq().if_(t);
    // 0 / 0 = 0
    sink.i32_const(0);
    sink.else_();
    // nonzero / 0 - saturate based on sign
    sink.local_get(lhs).i32_const(0).i32_lt_s().if_(t);
    // negative / 0 = MIN_FIXED
    sink.i32_const(MIN_FIXED);
    sink.else_();
    // positive / 0 = MAX_FIXED
    sink.i32_const(MAX_FIXED);
    sink.end();
    sink.end();
    sink.else_();
    // Divisor is non-zero - perform normal division
    sink.local_get(lhs)
        .i64_extend_i32_s()
        .i64_const(16)
        .i64_shl()
        .local_get(rhs)
        .i64_extend_i32_s()
        .i64_div_s()
        .i32_wrap_i64();
    sink.end();
    sink.local_set(dst);
}

pub(crate) fn emit_q32_fabs(sink: &mut InstructionSink<'_>, src: u32, dst: u32) {
    let t = BlockType::Result(ValType::I32);
    sink.local_get(src)
        .local_get(src)
        .i32_const(0)
        .i32_lt_s()
        .if_(t);
    sink.i32_const(0).local_get(src).i32_sub();
    sink.else_();
    sink.local_get(src);
    sink.end();
    sink.local_set(dst);
}

pub(crate) fn emit_q32_itof_s(sink: &mut InstructionSink<'_>, src: u32, dst: u32, scratch: u32) {
    sink.local_get(src)
        .i64_extend_i32_s()
        .i64_const(16)
        .i64_shl();
    emit_q32_sat_from_i64(sink, scratch);
    sink.local_set(dst);
}

pub(crate) fn emit_q32_itof_u(sink: &mut InstructionSink<'_>, src: u32, dst: u32, scratch: u32) {
    sink.local_get(src)
        .i64_extend_i32_u()
        .i64_const(16)
        .i64_shl();
    emit_q32_sat_from_i64(sink, scratch);
    sink.local_set(dst);
}

pub(crate) fn emit_q32_ffloor(sink: &mut InstructionSink<'_>, src: u32, dst: u32) {
    sink.local_get(src)
        .i32_const(16)
        .i32_shr_s()
        .i32_const(16)
        .i32_shl()
        .local_set(dst);
}

pub(crate) fn emit_q32_fceil(sink: &mut InstructionSink<'_>, src: u32, dst: u32) {
    sink.local_get(src)
        .i32_const(0xFFFF)
        .i32_add()
        .i32_const(16)
        .i32_shr_s()
        .i32_const(16)
        .i32_shl()
        .local_set(dst);
}

pub(crate) fn emit_q32_ftrunc(sink: &mut InstructionSink<'_>, src: u32, dst: u32) {
    let t = BlockType::Result(ValType::I32);
    sink.local_get(src)
        .i32_const(16)
        .i32_shr_s()
        .i32_const(16)
        .i32_shl()
        .local_set(dst);
    sink.local_get(src)
        .local_get(dst)
        .i32_ne()
        .local_get(src)
        .i32_const(0)
        .i32_lt_s()
        .i32_and()
        .if_(t);
    sink.local_get(dst).i32_const(0x1_0000).i32_add();
    sink.else_();
    sink.local_get(dst);
    sink.end();
    sink.local_set(dst);
}
