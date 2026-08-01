//! Native f32 subtraction — the f32 sibling of [`super::fsub_q32`].
//!
//! `-` is a `docs/design/float.md` §3 Guaranteed row (correctly rounded, RNE).

/// f32 subtraction: `a - b`.
#[unsafe(no_mangle)]
pub extern "C" fn __lp_lpir_fsub_f32(a: f32, b: f32) -> f32 {
    a - b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subtracts() {
        assert_eq!(__lp_lpir_fsub_f32(5.0, 1.25), 3.75);
    }

    #[test]
    fn inf_minus_inf_is_nan() {
        assert!(__lp_lpir_fsub_f32(f32::INFINITY, f32::INFINITY).is_nan());
    }
}
