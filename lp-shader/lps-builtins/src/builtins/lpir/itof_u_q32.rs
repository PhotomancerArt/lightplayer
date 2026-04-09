//! Unsigned integer (i32 bit pattern) → Q16.16 (matches `lpvm-cranelift` `emit_from_uint`).

const Q32_SHIFT: u32 = 16;
const MAX_INT: i32 = 32767;

/// `x` is a GLSL `uint` stored as i32 bits; clamp like Cranelift `emit_from_uint`.
#[unsafe(no_mangle)]
pub extern "C" fn __lp_lpir_itof_u_q32(x: i32) -> i32 {
    let clamped = if x < 0 { MAX_INT } else { x.min(MAX_INT) };
    clamped.wrapping_shl(Q32_SHIFT)
}
