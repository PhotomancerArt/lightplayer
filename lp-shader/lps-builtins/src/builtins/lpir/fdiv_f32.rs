//! Native f32 division — the f32 sibling of [`super::fdiv_q32`].
//!
//! `/` is a `docs/design/float.md` §3 Guaranteed row: correctly rounded (RNE),
//! `x/0 = ±inf` for finite non-zero `x`, `0/0 = NaN`. Nothing traps — the Q32
//! sibling's divide-by-zero guard has no f32 analogue and adding one would
//! contradict the spec.
//!
//! **On Xtensa** (the ESP32-S3's LX7, the classic's LX6) `a / b` compiles to a
//! call to `compiler_builtins`' software `__divsf3`, which the fps spike found
//! in every f32 shader's profile. The FPU has no divide instruction; its
//! divide is a sequence of estimate and fused steps (`div0.s`, `nexp01.s`,
//! `maddn.s`, `mkdadj.s`, `addexp(m).s`, `divn.s`). [`crate::f32_math::div`] runs the sequence
//! the Xtensa toolchain's own `__divsf3` uses, transcribed instruction for
//! instruction from its disassembly (`lp-xt/lp-xt-fp-harness`'s
//! `lp_fp_div_seq`, where the FP campaign checked it end to end on FS3 and
//! the emulator models it exactly: `docs/adr/2026-07-31-xtensa-fp-behavior-contract.md`
//! §5). Disassembly is output, not source (AGENTS.md's licence rule). It is
//! correctly rounded under the default rounding mode, which nothing changes.

/// f32 division: `a / b`.
#[unsafe(no_mangle)]
pub extern "C" fn __lp_lpir_fdiv_f32(a: f32, b: f32) -> f32 {
    crate::f32_math::div(a, b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn divides() {
        assert_eq!(__lp_lpir_fdiv_f32(10.0, 4.0), 2.5);
    }

    #[test]
    fn divide_by_zero_is_infinity_not_a_trap() {
        assert_eq!(__lp_lpir_fdiv_f32(1.0, 0.0), f32::INFINITY);
        assert_eq!(__lp_lpir_fdiv_f32(-1.0, 0.0), f32::NEG_INFINITY);
        assert!(__lp_lpir_fdiv_f32(0.0, 0.0).is_nan());
    }
}
