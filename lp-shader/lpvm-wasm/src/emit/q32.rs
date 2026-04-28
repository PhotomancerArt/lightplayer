//! Q16.16 fixed-point helpers for WASM emission.

use lps_q32::Q32_FRAC;
use wasm_encoder::{BlockType, InstructionSink, ValType};

use crate::emit::FdivRecipLocals;

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

/// Inline wrapping Q32 add: `i32.add` (modular arithmetic, no saturation).
/// Matches `lpvm-native` `AluRRR { Add }`. Selected when
/// `Q32Options::add_sub == Wrapping`.
pub(crate) fn emit_q32_fadd_wrap(sink: &mut InstructionSink<'_>, lhs: u32, rhs: u32, dst: u32) {
    sink.local_get(lhs).local_get(rhs).i32_add().local_set(dst);
}

/// Inline wrapping Q32 subtract: `i32.sub`.
pub(crate) fn emit_q32_fsub_wrap(sink: &mut InstructionSink<'_>, lhs: u32, rhs: u32, dst: u32) {
    sink.local_get(lhs).local_get(rhs).i32_sub().local_set(dst);
}

/// Inline wrapping Q32 multiply: `((a as i64 * b as i64) >> 16) as i32`,
/// modular semantics. Matches `lpvm-native`'s 5-VInst `mul/mulh/srli/slli/or`
/// expansion bit-for-bit.
pub(crate) fn emit_q32_fmul_wrap(sink: &mut InstructionSink<'_>, lhs: u32, rhs: u32, dst: u32) {
    sink.local_get(lhs)
        .i64_extend_i32_s()
        .local_get(rhs)
        .i64_extend_i32_s()
        .i64_mul()
        .i64_const(16)
        .i64_shr_s()
        .i32_wrap_i64()
        .local_set(dst);
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

/// Reciprocal fast divide — matches `__lp_lpir_fdiv_recip_q32` (phase 2 helper) bit-for-bit.
pub(crate) fn emit_q32_fdiv_recip(
    sink: &mut InstructionSink<'_>,
    lhs: u32,
    rhs: u32,
    dst: u32,
    s: &FdivRecipLocals,
) {
    let t = BlockType::Result(ValType::I32);
    let d = s.dividend;
    let r = s.divisor;
    let sign = s.sign;
    let abs_dividend = s.abs_dividend;
    let abs_divisor = s.abs_divisor;
    let recip = s.recip;
    let quot = s.quot;

    // Stack: dividend (lhs), divisor (rhs) — top is rhs.
    sink.local_get(lhs).local_get(rhs);
    sink.local_set(r);
    sink.local_set(d);

    sink.local_get(r).i32_eqz().if_(t);
    // divisor == 0 — match `emit_q32_fdiv` saturation policy.
    sink.local_get(d).i32_const(0).i32_eq().if_(t);
    sink.i32_const(0);
    sink.else_();
    sink.local_get(d).i32_const(0).i32_gt_s().if_(t);
    sink.i32_const(MAX_FIXED);
    sink.else_();
    sink.i32_const(MIN_FIXED);
    sink.end();
    sink.end();
    sink.else_();
    // Non-zero divisor: reciprocal multiply.
    sink.local_get(d)
        .local_get(r)
        .i32_xor()
        .i32_const(0)
        .i32_lt_s()
        .if_(t);
    sink.i32_const(-1);
    sink.else_();
    sink.i32_const(1);
    sink.end();
    sink.local_set(sign);

    sink.local_get(d);
    sink.local_get(d);
    sink.i32_const(31).i32_shr_s();
    sink.i32_xor();
    sink.local_get(d);
    sink.i32_const(31).i32_shr_s();
    sink.i32_sub();
    sink.local_set(abs_dividend);

    sink.local_get(r);
    sink.local_get(r);
    sink.i32_const(31).i32_shr_s();
    sink.i32_xor();
    sink.local_get(r);
    sink.i32_const(31).i32_shr_s();
    sink.i32_sub();
    sink.local_set(abs_divisor);

    sink.i32_const(i32::MIN);
    sink.local_get(abs_divisor);
    sink.i32_div_u();
    sink.local_set(recip);

    sink.local_get(abs_dividend);
    sink.i64_extend_i32_u();
    sink.local_get(recip);
    sink.i64_extend_i32_u();
    sink.i64_mul();
    sink.i64_const(1);
    sink.i64_shl();
    sink.i64_const(16);
    sink.i64_shr_u();
    sink.i32_wrap_i64();
    sink.local_set(quot);

    sink.local_get(quot).local_get(sign).i32_mul();
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

/// Q16.16 → `int` (truncate toward zero). Matches [`lps_builtins::...::ftoi_sat_q32::__lp_lpir_ftoi_sat_s_q32`]
/// and [`lpvm_cranelift::q32_emit::emit_to_sint`].
pub(crate) fn emit_q32_ftoi_sat_s(sink: &mut InstructionSink<'_>, src: u32, dst: u32) {
    let t = BlockType::Result(ValType::I32);
    sink.local_get(src).i32_const(0).i32_lt_s().if_(t);
    sink.local_get(src).i32_const(Q32_FRAC).i32_add();
    sink.else_();
    sink.local_get(src);
    sink.end();
    sink.i32_const(16).i32_shr_s().local_set(dst);
}

/// Q16.16 → `uint` (negative → 0). Matches `__lp_lpir_ftoi_sat_u_q32`.
pub(crate) fn emit_q32_ftoi_sat_u(sink: &mut InstructionSink<'_>, src: u32, dst: u32) {
    let t = BlockType::Result(ValType::I32);
    emit_q32_ftoi_sat_s(sink, src, dst);
    sink.local_get(dst).i32_const(0).i32_lt_s().if_(t);
    sink.i32_const(0);
    sink.else_();
    sink.local_get(dst);
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

/// `uint` → Q16.16, matching [`lpvm_cranelift::q32_emit::emit_from_uint`].
/// `scratch` is the function's i64 slot (Q32 fadd path); we store a sign-extended `i32` there.
pub(crate) fn emit_q32_itof_u(sink: &mut InstructionSink<'_>, src: u32, dst: u32, scratch: u32) {
    const MAX_I32_PER_Q32: i32 = 32767;
    const SMIN_BOUND: i32 = 0x8000; // first i32 not representable as Q32 int mag; `v < this` keeps `v` in smin
    let t = BlockType::Result(ValType::I32);
    sink.local_get(src);
    sink.i32_const(SMIN_BOUND);
    sink.i32_lt_s();
    sink.if_(t);
    sink.local_get(src);
    sink.else_();
    sink.i32_const(MAX_I32_PER_Q32);
    sink.end();
    sink.i64_extend_i32_s();
    sink.local_set(scratch);
    // (src < 0) ? MAX : smin
    sink.local_get(src);
    sink.i32_const(0);
    sink.i32_lt_s();
    sink.if_(t);
    sink.i32_const(MAX_I32_PER_Q32);
    sink.else_();
    sink.local_get(scratch);
    sink.i32_wrap_i64();
    sink.end();
    sink.i32_const(16);
    sink.i32_shl();
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
