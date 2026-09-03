//! Fixed-point 16.16 exponential function.
//!
//! `exp(x)` for `x > 0` is the Maclaurin series `1 + x + x²/2! + x³/3! + …`
//! evaluated term by term in Q16.16 (`term_{n} = term_{n-1} · x / n`, each
//! product saturating like [`__lp_lpir_fmul_q32`]), stopping once a term drops
//! below 500/65536 (after the 15th term) or below 20/65536. Negative
//! arguments use `exp(-x) = 1 / exp(x)`; the result saturates to `i32::MAX`
//! above ≈ 10.4 and underflows to `0` below ≈ −11.8.
//!
//! ## 32-bit arithmetic, bit-identical to the former i64 form (2026-09-02)
//!
//! The series used to divide and take the reciprocal with the saturating
//! Q16.16 divide (`(a << 16) / b` in `i64`), which on RV32 is a
//! `__udivdi3` call per term — ~164 cycles each on the ESP32-C6 model, ~80%
//! of the builtin's cost. Both divides reduce exactly to 32-bit ones:
//!
//! - `fdiv_q32(in_value, n << 16)` with `in_value > 0` divides two positive
//!   multiples of 2^16, so the truncating quotient is exactly `in_value / n`;
//!   it is at most `in_value / 2` and never saturates. That `u32` division is
//!   itself replaced by an exact reciprocal multiply: with `M = ⌈2^32 / n⌉`
//!   and `e = M·n − 2^32` (so `0 ≤ e < n`), `⌊x·M / 2^32⌋ = ⌊x / n⌋` whenever
//!   `x·e < 2^32` — write `x = q·n + r` and the fractional part is
//!   `r/n + x·e/(n·2^32) < 1`. Here `x ≤ 772 242 < 2^20` and `e < 29`, so the
//!   product is under 2^25. The condition is checked for every table entry at
//!   compile time. On RV32M the multiply is one `mulhu` (vs 32 cycles for
//!   `divu`); on every other target it is an ordinary 64-bit product with the
//!   same value.
//! - `fdiv_q32(ONE, r)` is `floor(2^32 / r)`. Here `r ≥ 65537` (the series
//!   starts at `in_value + ONE` with `in_value ≥ 1` and only adds
//!   non-negative terms), so the quotient fits in 16 bits and never
//!   saturates. For `1 ≤ r ≤ 2^31`, `floor(2^32 / r) = floor((2^32 − 1) / r)`
//!   unless `r` divides 2^32, i.e. unless `r` is a power of two, in which
//!   case it is one larger.
//!
//! Everything else (the saturating product, the saturating accumulation, the
//! termination test, the special cases) is unchanged. Equality with the
//! previous implementation was proven for **every `i32` input** by the
//! exhaustive test at the bottom of this file (run 2026-09-02; it keeps the
//! former body as the reference).
//!
//! The algorithm is the textbook power series; no code was ported from any
//! external library.

use crate::builtins::lpir::fmul_q32::fmul_q32_sat;

/// Fixed-point value of 1.0 (Q16.16 format)
const FIX16_ONE: i32 = 0x00010000; // 65536
/// Fixed-point value of e (Q16.16 format)
const FIX16_E: i32 = 178145;
/// Maximum value before overflow
const FIX16_MAX_EXP: i32 = 681391; // ~10.4 in fixed point
/// Minimum value before underflow
const FIX16_MIN_EXP: i32 = -772243; // ~-11.8 in fixed point
/// `fdiv_q32(ONE, i32::MAX)` = `floor(2^32 / (2^31 − 1))`: the reciprocal of
/// a saturated `exp(|x|)`, used when `x ≤ −FIX16_MAX_EXP`.
const RECIP_OF_SATURATED: i32 = 2;
/// Largest `|x|` the series ever sees: `-(FIX16_MIN_EXP + 1)`.
const IN_VALUE_MAX: u32 = 772_242;
/// Series terms `2..SERIES_TERMS` (exclusive), as before.
const SERIES_TERMS: usize = 30;
/// `⌈2^32 / n⌉` for `n in 2..SERIES_TERMS`, the exact-quotient multipliers
/// for `in_value / n` (module doc). Entries 0 and 1 are unused.
const RECIP_TABLE: [u32; SERIES_TERMS] = build_recip_table();

const fn build_recip_table() -> [u32; SERIES_TERMS] {
    let mut table = [0u32; SERIES_TERMS];
    let mut n = 2;
    while n < SERIES_TERMS {
        table[n] = (1u64 << 32).div_ceil(n as u64) as u32;
        n += 1;
    }
    table
}

// Exactness condition `x·e < 2^32` for every divisor and the largest `x`
// (see the module doc); a violation is a compile error, not a wrong pixel.
const _: () = {
    let mut n = 2;
    while n < SERIES_TERMS {
        let e = RECIP_TABLE[n] as u64 * n as u64 - (1u64 << 32);
        assert!(e * (IN_VALUE_MAX as u64) < (1u64 << 32));
        n += 1;
    }
};

/// Compute `exp(x)` in Q16.16. See the module doc for the algorithm and the
/// bit-identity argument for its 32-bit divides.
#[unsafe(no_mangle)]
pub extern "C" fn __lps_exp_q32(x: i32) -> i32 {
    // Handle special cases
    if x == 0 {
        return FIX16_ONE;
    }
    if x == FIX16_ONE {
        return FIX16_E;
    }
    if x >= FIX16_MAX_EXP {
        return i32::MAX; // Saturate to maximum
    }
    if x <= FIX16_MIN_EXP {
        return 0; // Underflow to zero
    }

    // The power-series converges much faster on positive values
    // and exp(-x) = 1/exp(x).
    let neg = x < 0;
    if neg && x <= -FIX16_MAX_EXP {
        return RECIP_OF_SATURATED;
    }
    // `x` is nonzero and above FIX16_MIN_EXP, so `in_value` is in
    // `1..=772242` here — strictly positive, which is what both 32-bit
    // divides below rely on.
    let in_value = if neg { x.saturating_neg() } else { x };
    debug_assert!(in_value >= 1 && in_value as u32 <= IN_VALUE_MAX);
    let in_value_u = in_value as u64;

    let mut result = in_value + FIX16_ONE;
    let mut term = in_value;

    // Compute power series: term_n+1 = term_n * x / n
    //
    // Indexed on purpose: the `iter().enumerate().skip(2)` form the lint
    // suggests measured 998 vs 760 cycles at x = -6 on the C6 model (the
    // iterator loop is laid out worse by LLVM at the image's opt level).
    #[allow(
        clippy::needless_range_loop,
        reason = "measured 30% slower as an iterator loop"
    )]
    for i in 2..SERIES_TERMS {
        // Exactly `fdiv_q32(in_value, i << 16)` = `in_value / i`, by the
        // reciprocal table (module doc). One `mulhu` on RV32M.
        let x_over_i = ((in_value_u * RECIP_TABLE[i] as u64) >> 32) as i32;
        term = fmul_q32_sat(term, x_over_i);
        result = result.saturating_add(term);

        // Early termination if term becomes small enough
        if (term < 500) && ((i > 15) || (term < 20)) {
            break;
        }
    }

    // Handle negative x: exp(-x) = 1/exp(x)
    if neg {
        result = recip_one_q32(result);
    }

    result
}

/// `floor(2^32 / r)` for `r ≥ 1`, i.e. exactly `fdiv_q32(ONE, r)` whenever
/// that quotient fits (it does for every `r ≥ 2`; `exp` only calls this with
/// `r ≥ 65537`). See the module doc for why the power-of-two adjustment is
/// exact.
#[inline(always)]
fn recip_one_q32(r: i32) -> i32 {
    let r = r as u32;
    (u32::MAX / r + (r.is_power_of_two() as u32)) as i32
}

#[cfg(test)]
mod tests {
    #[cfg(test)]
    extern crate std;
    use super::*;
    use crate::util::test_helpers::{fixed_to_float, float_to_fixed, test_q32_function_relative};

    #[test]
    fn test_exp_basic() {
        let tests = [
            (0.0, 1.0),
            (1.0, 2.718281828459045),    // e
            (-1.0, 0.36787944117144233), // 1/e
            (2.0, 7.38905609893065),     // e²
            (0.5, 1.6487212707001282),   // sqrt(e)
        ];

        // Use 3% tolerance for exponential functions
        test_q32_function_relative(|x| __lps_exp_q32(x), &tests, 0.03, 0.01);
    }

    #[test]
    fn test_exp_large_negative_does_not_panic() {
        assert_eq!(__lps_exp_q32(FIX16_MIN_EXP), 0);
        assert!(fixed_to_float(__lps_exp_q32(float_to_fixed(-11.0))) <= 0.001);
    }

    #[test]
    fn test_exp_large_positive_saturates() {
        assert_eq!(__lps_exp_q32(float_to_fixed(11.0)), i32::MAX);
    }

    /// Every boundary constant ±2, the whole series domain stepped by 7, and
    /// 100k LCG-random inputs must match the former i64 implementation.
    #[test]
    fn sampled_bit_identical_to_reference() {
        let mut inputs: std::vec::Vec<i32> = std::vec::Vec::new();
        for c in [
            0,
            FIX16_ONE,
            FIX16_MAX_EXP,
            -FIX16_MAX_EXP,
            FIX16_MIN_EXP,
            i32::MAX,
            i32::MIN,
            1,
            -1,
        ] {
            for d in -2..=2 {
                inputs.push(c.wrapping_add(d));
            }
        }
        inputs.extend((FIX16_MIN_EXP - 64..=FIX16_MAX_EXP + 64).step_by(7));
        let mut lcg: u32 = 0x9E37_79B9;
        for _ in 0..100_000 {
            lcg = lcg.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            inputs.push(lcg as i32);
        }
        for x in inputs {
            assert_eq!(
                __lps_exp_q32(x),
                exp_q32_reference(x),
                "exp_q32 diverges from the reference at x = {x}"
            );
        }
    }

    /// The proof: all 2^32 inputs against the former implementation.
    ///
    /// ```bash
    /// cargo test -p lps-builtins --release exp_q32::tests::exhaustive -- --ignored --nocapture
    /// ```
    #[test]
    #[ignore = "exhaustive 2^32-input proof; run explicitly in --release"]
    fn exhaustive_bit_identical_to_reference() {
        let threads = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4);
        let started = std::time::Instant::now();
        let chunk = (1u64 << 32).div_ceil(threads as u64);
        let mismatch = std::sync::Mutex::new(None::<i32>);
        std::thread::scope(|scope| {
            for t in 0..threads as u64 {
                let mismatch = &mismatch;
                scope.spawn(move || {
                    let lo = t * chunk;
                    let hi = ((t + 1) * chunk).min(1u64 << 32);
                    for raw in lo..hi {
                        let x = raw as u32 as i32;
                        if __lps_exp_q32(x) != exp_q32_reference(x) {
                            *mismatch.lock().unwrap() = Some(x);
                            return;
                        }
                    }
                });
            }
        });
        let elapsed = started.elapsed();
        if let Some(x) = *mismatch.lock().unwrap() {
            panic!(
                "exp_q32 diverges from the reference at x = {x}: new {} vs reference {}",
                __lps_exp_q32(x),
                exp_q32_reference(x)
            );
        }
        std::println!(
            "exp_q32: 4294967296 inputs bit-identical to the reference in {:.1?} on {threads} threads",
            elapsed
        );
    }

    /// The implementation this file replaced (2026-09-02), kept verbatim as
    /// the oracle for the equivalence tests above, with private copies of the
    /// saturating i64 helpers it used so it does not depend on the refactored
    /// production helpers.
    fn exp_q32_reference(x: i32) -> i32 {
        if x == 0 {
            return FIX16_ONE;
        }
        if x == FIX16_ONE {
            return FIX16_E;
        }
        if x >= FIX16_MAX_EXP {
            return i32::MAX;
        }
        if x <= FIX16_MIN_EXP {
            return 0;
        }
        let neg = x < 0;
        if neg && x <= -FIX16_MAX_EXP {
            return ref_fdiv_q32(FIX16_ONE, i32::MAX);
        }
        let in_value = if neg { x.saturating_neg() } else { x };
        let mut result = in_value + FIX16_ONE;
        let mut term = in_value;
        for i in 2..30 {
            let i_fixed = i << 16;
            term = ref_fmul_q32(term, ref_fdiv_q32(in_value, i_fixed));
            result = result.saturating_add(term);
            if (term < 500) && ((i > 15) || (term < 20)) {
                break;
            }
        }
        if neg {
            result = ref_fdiv_q32(FIX16_ONE, result);
        }
        result
    }

    fn ref_fdiv_q32(dividend: i32, divisor: i32) -> i32 {
        const MAX_FIXED: i32 = 0x7FFF_FFFF;
        const MIN_FIXED: i32 = i32::MIN;
        if divisor == 0 {
            return if dividend == 0 {
                0
            } else if dividend > 0 {
                MAX_FIXED
            } else {
                MIN_FIXED
            };
        }
        let result_wide = ((dividend as i64) << 16) / (divisor as i64);
        if result_wide > MAX_FIXED as i64 {
            MAX_FIXED
        } else if result_wide < MIN_FIXED as i64 {
            MIN_FIXED
        } else {
            result_wide as i32
        }
    }

    fn ref_fmul_q32(a: i32, b: i32) -> i32 {
        const MAX_FIXED: i32 = 0x7FFF_FFFF;
        const MIN_FIXED: i32 = i32::MIN;
        if a == 0 || b == 0 {
            return 0;
        }
        let shifted_wide = ((a as i64) * (b as i64)) >> 16;
        if shifted_wide > MAX_FIXED as i64 {
            MAX_FIXED
        } else if shifted_wide < MIN_FIXED as i64 {
            MIN_FIXED
        } else {
            shifted_wide as i32
        }
    }
}
