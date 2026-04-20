//! Compile-time-sized gradient LUT for `psrdnoise2_q32` (`hash * 0.07482` × unit circle).
//!
//! **Size:** 289 × (`cos`, `sin`) × 4 B ≈ 2312 B in rodata. Each entry matches
//! [`lps_sincos_q32_pair`](crate::builtins::glsl::sincos_q32::lps_sincos_q32_pair)`((hash as i32) * 4904)` so the
//! Taylor path stays bit-identical to per-corner `sincos` without runtime init.
//!
//! The array lives in [`grad_lut_q32_data`](super::grad_lut_q32_data) (regenerate via ignored test in this module).

use super::grad_lut_q32_data::GRAD_COS_SIN_LUT;

/// Lookup `(cos ψ_h, sin ψ_h)` before α rotation (`ψ = hash * 0.07482` in Q16.16, `hash ∈ [0, 288]`).
#[inline(always)]
pub fn grad_base_cos_sin(hash: i32) -> (i32, i32) {
    let i = hash.clamp(0, 288) as usize;
    GRAD_COS_SIN_LUT[i]
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::GRAD_COS_SIN_LUT;
    use crate::builtins::glsl::sincos_q32::lps_sincos_q32_pair;

    /// `0.07482 * 65536` — must match `psrdnoise2` hash scale.
    const HASH_MULT_0_07482_RAW: i32 = 4904;

    #[test]
    fn grad_lut_matches_sincos_pair() {
        for h in 0..289 {
            let base = (h as i32).wrapping_mul(HASH_MULT_0_07482_RAW);
            let (sin_b, cos_b) = lps_sincos_q32_pair(base);
            assert_eq!(
                GRAD_COS_SIN_LUT[h],
                (cos_b, sin_b),
                "LUT mismatch at hash index {h}"
            );
        }
    }

    /// Regenerate `grad_lut_q32_data.rs` `GRAD_COS_SIN_LUT`:
    /// `cargo test -p lps-builtins emit_grad_lut_entries -- --ignored --nocapture`, then wrap the printed `[`…`]` as
    /// `pub(super) const GRAD_COS_SIN_LUT: [(i32, i32); 289] = ...;` in that module.
    #[test]
    #[ignore]
    fn emit_grad_lut_entries() {
        std::println!("[");
        for h in 0..289 {
            let base = (h as i32).wrapping_mul(HASH_MULT_0_07482_RAW);
            let (sin_b, cos_b) = lps_sincos_q32_pair(base);
            let comma = if h == 288 { "" } else { "," };
            std::println!("    ({cos_b}, {sin_b}){comma}");
        }
        std::println!("]");
    }
}
