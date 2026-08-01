//! Native f32 `fma` — the f32 sibling of [`super::fma_q32`].
//!
//! **A single rounding, and that is the whole point.** `docs/design/float.md`
//! §3 makes explicitly-spelled `fma` a Guaranteed row: `a * b + c` computed
//! with one rounding of the exact product-plus-addend. Writing `a * b + c` here
//! would round twice and quietly violate the row — and §4 separately makes
//! *contraction* of a separate multiply-and-add target-defined, so the two
//! spellings genuinely mean different things.
//!
//! The Q32 sibling's doc comment says it "can't truly fuse the operations."
//! f32 can, so it must.
//!
//! **Tolerance:** exact (correctly rounded).

/// GLSL `fma(a, b, c)` — `a * b + c` with a single rounding.
#[unsafe(no_mangle)]
pub extern "C" fn __lps_fma_f32(a: f32, b: f32, c: f32) -> f32 {
    libm::fmaf(a, b, c)
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;

    #[test]
    fn ordinary_cases() {
        assert_eq!(__lps_fma_f32(2.0, 3.0, 4.0), 10.0);
        assert_eq!(__lps_fma_f32(-2.0, 3.0, 1.0), -5.0);
        assert_eq!(__lps_fma_f32(0.5, 0.5, 0.0), 0.25);
    }

    /// The property that separates `fma` from `a * b + c`: with one rounding
    /// the exact f64 answer survives, with two it does not.
    #[test]
    fn single_rounding_beats_multiply_then_add() {
        // a*b is inexact in f32; the addend cancels most of it, so the
        // double-rounded form loses the low bits the fused form keeps.
        let a = 1.0f32 + f32::EPSILON;
        let b = 1.0f32 - f32::EPSILON;
        let c = -1.0f32;
        let fused = __lps_fma_f32(a, b, c);
        let exact = ((a as f64) * (b as f64) + c as f64) as f32;
        assert_eq!(fused, exact, "fma must match the exactly-rounded result");
        assert_ne!(fused, a * b + c, "a*b+c should double-round here");
    }
}
