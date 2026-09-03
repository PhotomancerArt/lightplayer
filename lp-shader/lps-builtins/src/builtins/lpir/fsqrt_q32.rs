//! Fixed-point 16.16 square root — exact floor root with 32-bit arithmetic.
//!
//! `__lp_lpir_fsqrt_q32(x)` returns `floor(sqrt(x · 2^16))` for `x > 0` and `0`
//! for `x <= 0`, i.e. the Q16.16 square root truncated towards zero. That is
//! exactly what `((x as u64) << 16).isqrt()` used to compute; this module keeps
//! the value bit-identical while spending no 64-bit division, no 64-bit shift
//! and no library call — only 32-bit ALU ops plus 32×32→64 *products*, which
//! RV32M provides as a `mul`/`mulhu` pair.
//!
//! Everything below is derived from first principles (Newton's iteration for
//! the reciprocal square root, plus the uniqueness of the integer floor root);
//! no code or table was ported from another implementation.
//!
//! # Algorithm
//!
//! Let `n = x · 2^16` (a 47-bit value) and `r = floor(sqrt(n))` (< 2^24).
//!
//! 1. **Normalise.** Shift `x` left by an *even* amount `s = 2k` so that
//!    `m = x << s` lands in `[2^30, 2^32)`. The shift ladder tests
//!    `2^16 / 2^24 / 2^28 / 2^30` and shifts by `16 / 8 / 4 / 2`, so it needs
//!    four steps rather than the five of a general `clz` (RV32IMAC has no
//!    `clz` instruction). Because `16 - s` is even,
//!    `sqrt(n) = sqrt(m) · 2^(8 - k)` *exactly*.
//! 2. **Seed.** With `M = m / 2^32 ∈ [1/4, 1)`, look up an approximation of
//!    `R = 1/sqrt(M) ∈ (1, 2]` in a 256-entry table indexed by the top eight
//!    bits of `m`. The entry is `R` in Q15; the working value is Q30. A bucket
//!    spans at most 2^-6 relative in `m`, so the seeded `R` is good to about
//!    2^-8 relative.
//! 3. **Refine.** Two Newton steps of `R ← R · (3 − M·R²) / 2`, every product
//!    taken as the high word of a 32×32→64 multiply. The iteration is
//!    quadratic and one-sided — `R_new = R* · (1 − 1.5ε² − ε³/2)` for
//!    `R = R*(1 + ε)` — so it converges *from below*: 2^-8 → 2^-15 → 2^-26,
//!    where the last figure is set by the fixed-point rounding rather than by
//!    the iteration. Each step also subtracts one Q26 unit, which bounds the
//!    rounding of the intermediates and makes `R ≤ 1/sqrt(M)` unconditional.
//! 4. **Candidate.** `y = mulhi(m, R) << 2` is `sqrt(m) · 2^16` to within
//!    about 75 units low and never high, so `y < 2^32` cannot overflow, and
//!    the candidate at final scale is `c = y >> (8 + k)`.
//! 5. **Correct.** `c` is exact or one too small, so walking `c` up while
//!    `(c+1)² ≤ n` lands on the unique floor root. `(c+1)²` and `n` are
//!    compared as `(hi, lo)` pairs of 32-bit words — `n` is `(x >> 16,
//!    x << 16)` and the square is `(mulhi(c+1, c+1), (c+1)·(c+1))`.
//!
//! # Why the result is exact
//!
//! `floor(sqrt(n))` is the unique `c ≥ 0` with `c² ≤ n < (c+1)²`. Step 5 exits
//! only when `(c+1)² > n`, and it starts from a `c` that satisfies `c² ≤ n`
//! (it is never above the true root, since `y` is never above `sqrt(m)·2^16`
//! and truncating a right shift only lowers it further). So the loop's exit
//! state *is* the definition of the floor root, independently of how good the
//! approximation was — approximation quality only decides how many times the
//! loop iterates. The exhaustive proof below reports that maximum.
//!
//! # Seed table
//!
//! `RSQRT_SEED_Q15[i] = round(2^15 / sqrt((i + 0.5) / 256))
//!                    = round(2^19 / sqrt(i + 0.5))
//!                    = round(sqrt(2^38 / (i + 0.5)))`,
//! computed at compile time by `rsqrt_seed_entry` with integer arithmetic
//! only (`round(sqrt(v)) = (1 + isqrt(4v)) >> 1`, so the const fn evaluates
//! `4v = 2^41 / (2i + 1)` directly). Normalisation guarantees
//! `i = m >> 24 ≥ 64`, so entries `0..64` are unreachable and hold `u16::MAX`.
//! The table is 512 bytes of rodata.
//!
//! # Proof
//!
//! `exhaustive_bit_identical_to_reference` compared this implementation with
//! `((x as u64) << 16).isqrt()` for **all 2^32 `i32` inputs** on 2026-09-02:
//! zero mismatches, at most **one** correction step, and the candidate never
//! overshot the true root.

/// Q15 approximations of `1/sqrt(M)` for `M = (i + 0.5) / 256`.
///
/// See the module docs: index `i` is the top byte of the normalised mantissa,
/// which is always `>= 64`; the low entries exist only to make the index
/// provably in range (so the lookup carries no bounds check).
const RSQRT_SEED_Q15: [u16; 256] = {
    let mut table = [0u16; 256];
    let mut i = 0;
    while i < 256 {
        table[i] = rsqrt_seed_entry(i);
        i += 1;
    }
    table
};

/// Compute square root in Q16.16 fixed point.
///
/// Returns `floor(sqrt(x · 2^16))` for `x > 0`, and `0` for `x <= 0`.
#[unsafe(no_mangle)]
pub extern "C" fn __lp_lpir_fsqrt_q32(x: i32) -> i32 {
    if x <= 0 {
        return 0;
    }
    let x = x as u32;

    // Never above the true root (see module docs), so only upward corrections
    // are possible.
    let mut root = fsqrt_candidate(x);

    // n = x * 2^16 as a (hi, lo) pair of 32-bit words.
    let n_hi = x >> 16;
    let n_lo = x << 16;

    loop {
        let next = root + 1;
        let sq_lo = next.wrapping_mul(next);
        let sq_hi = mul_hi_u32(next, next);
        if sq_hi > n_hi || (sq_hi == n_hi && sq_lo > n_lo) {
            break;
        }
        root = next;
    }

    root as i32
}

/// Approximate `floor(sqrt(x · 2^16))` from below, within one unit.
///
/// `x` must be non-zero. Split out from [`__lp_lpir_fsqrt_q32`] so the proof
/// tests can measure how many correction steps the candidate leaves behind.
#[inline(always)]
fn fsqrt_candidate(x: u32) -> u32 {
    // Normalise: m = x << 2k lands in [2^30, 2^32), and sqrt(x * 2^16) is then
    // sqrt(m) * 2^(8 - k).
    let mut m = x;
    let mut k = 0u32;
    let sh = if m < 1 << 16 { 16 } else { 0 };
    m <<= sh;
    k += sh >> 1;
    let sh = if m < 1 << 24 { 8 } else { 0 };
    m <<= sh;
    k += sh >> 1;
    let sh = if m < 1 << 28 { 4 } else { 0 };
    m <<= sh;
    k += sh >> 1;
    let sh = if m < 1 << 30 { 2 } else { 0 };
    m <<= sh;
    k += sh >> 1;

    // Reciprocal square root of M = m / 2^32, in Q30, refined from below.
    let mut recip = u32::from(RSQRT_SEED_Q15[(m >> 24) as usize]) << 15;
    recip = rsqrt_newton(m, recip);
    recip = rsqrt_newton(m, recip);

    // m * (2^16 / sqrt(m)) = sqrt(m) * 2^16, in Q0 within [2^31, 2^32).
    let scaled_root = mul_hi_u32(m, recip) << 2;

    scaled_root >> (8 + k)
}

/// One Newton step of `R <- R * (3 - M * R^2) / 2`, with `M = m / 2^32` and
/// `R` in Q30.
///
/// The `- 1` is a Q26 unit taken off the result: it costs about 2^-27 relative
/// and guarantees the returned `R` stays at or below the true `1/sqrt(M)`
/// however the intermediate truncations fall.
#[inline(always)]
fn rsqrt_newton(m: u32, recip: u32) -> u32 {
    let sq = mul_hi_u32(recip, recip); // Q28: R^2
    let m_sq = mul_hi_u32(m, sq); // Q28: M * R^2, close to 1
    let residual = (3 << 28) - m_sq; // Q28: 3 - M * R^2, close to 2
    (mul_hi_u32(recip, residual) - 1) << 3 // Q30
}

/// High word of the 32x32 -> 64 unsigned product (`mulhu` on RV32M).
#[inline(always)]
fn mul_hi_u32(a: u32, b: u32) -> u32 {
    (((a as u64) * (b as u64)) >> 32) as u32
}

/// `round(2^19 / sqrt(i + 0.5))` as Q15, or `u16::MAX` for the unreachable
/// entries below 64. See the module docs for the derivation.
const fn rsqrt_seed_entry(i: usize) -> u16 {
    if i < 64 {
        return u16::MAX;
    }
    // round(sqrt(v)) == (1 + isqrt(4v)) >> 1, with v = 2^38 / (i + 0.5).
    let four_v = (1u64 << 41) / (2 * i as u64 + 1);
    ((1 + four_v.isqrt()) >> 1) as u16
}

#[cfg(test)]
mod tests {
    #[cfg(test)]
    extern crate std;
    use super::*;

    #[test]
    fn test_perfect_squares() {
        let tests = [
            (0.0, 0.0),
            (1.0, 1.0),
            (4.0, 2.0),
            (9.0, 3.0),
            (16.0, 4.0),
            (25.0, 5.0),
            (100.0, 10.0),
        ];

        for (x, expected) in tests {
            let x_fixed = float_to_fixed(x);
            let result_fixed = __lp_lpir_fsqrt_q32(x_fixed);
            let result = fixed_to_float(result_fixed);

            std::println!("sqrt({}) -> Expected: {}, Actual: {}", x, expected, result);

            assert!(
                (result - expected).abs() < 0.01,
                "sqrt({}) failed: expected {}, got {}",
                x,
                expected,
                result
            );
        }
    }

    #[test]
    fn test_non_perfect_squares() {
        let tests = [
            (2.0, 1.4142135623730951),
            (3.0, 1.7320508075688772),
            (5.0, 2.23606797749979),
            (0.25, 0.5),
            (0.5, 0.7071067811865476),
        ];

        for (x, expected) in tests {
            let x_fixed = float_to_fixed(x);
            let result_fixed = __lp_lpir_fsqrt_q32(x_fixed);
            let result = fixed_to_float(result_fixed);

            std::println!(
                "sqrt({}) -> Expected: {}, Actual: {}, Error: {}",
                x,
                expected,
                result,
                (result - expected).abs()
            );

            // Allow 2% error tolerance
            let tolerance = (expected.max(0.01f32)) * 0.02;
            assert!(
                (result - expected).abs() < tolerance,
                "sqrt({}) failed: expected {}, got {}",
                x,
                expected,
                result
            );
        }
    }

    #[test]
    fn test_edge_cases() {
        // Test x <= 0 should return 0
        assert_eq!(__lp_lpir_fsqrt_q32(0), 0, "sqrt(0) should be 0");
        assert_eq!(__lp_lpir_fsqrt_q32(-1), 0, "sqrt(-1) should be 0");
        assert_eq!(__lp_lpir_fsqrt_q32(i32::MIN), 0, "sqrt(MIN) should be 0");
    }

    #[test]
    fn test_large_values() {
        let tests = [(1000.0, 31.622776601683793), (10000.0, 100.0)];

        for (x, expected) in tests {
            let x_fixed = float_to_fixed(x);
            let result_fixed = __lp_lpir_fsqrt_q32(x_fixed);
            let result = fixed_to_float(result_fixed);

            std::println!(
                "sqrt({}) -> Expected: {}, Actual: {}, Error: {}",
                x,
                expected,
                result,
                (result - expected).abs()
            );

            // Allow larger error tolerance for very large values
            let tolerance = if x > 5000.0 {
                (expected.max(0.01f32)) * 0.6 // 60% tolerance for very large values
            } else {
                (expected.max(0.01f32)) * 0.02 // 2% tolerance for normal values
            };
            assert!(
                (result - expected).abs() < tolerance,
                "sqrt({}) failed: expected {}, got {}",
                x,
                expected,
                result
            );
        }
    }

    /// Guard for the exhaustive proof: the structured edge cases plus a random
    /// sweep, cheap enough to run in the default (debug) suite.
    #[test]
    fn sampled_bit_identical_to_reference() {
        let started = std::time::Instant::now();
        let mut inputs = 0u64;
        let mut max_corrections = 0i64;
        let mut check = |x: i32| {
            inputs += 1;
            let corrections = check_against_reference(x);
            if corrections > max_corrections {
                max_corrections = corrections;
            }
        };

        // Every small input, where the normalisation shift is largest.
        for x in 1..=(1i32 << 20) {
            check(x);
        }

        // Powers of two and their neighbours: every normalisation bucket edge.
        for bit in 0..31 {
            let p = 1i32 << bit;
            check(p - 1);
            check(p);
            check(p + 1);
        }

        // Perfect squares (n = x * 2^16 is then an exact square, the hardest
        // place for a floor root) and their neighbours. j = k * 2^8 is the
        // subset whose root is a whole Q16.16 unit.
        let mut j = 1i64;
        while j * j <= i64::from(i32::MAX) {
            let sq = (j * j) as i32;
            check(sq - 1);
            check(sq);
            check(sq + 1);
            j += 1;
        }

        check(i32::MAX);
        check(i32::MIN);

        // 200k pseudo-random inputs across the whole i32 range.
        let mut state = 0x2545_F491_4F6C_DD1Du64;
        for _ in 0..200_000 {
            state = next_lcg(state);
            check((state >> 32) as u32 as i32);
        }

        std::println!(
            "sampled_bit_identical_to_reference: {inputs} inputs, max {max_corrections} correction step(s), {:?}",
            started.elapsed()
        );
    }

    /// The proof itself: every `i32` input, split across all available cores.
    ///
    /// `cargo test -p lps-builtins --release \
    ///     fsqrt_q32::tests::exhaustive -- --ignored --nocapture`
    #[test]
    #[ignore = "exhaustive proof over all 2^32 i32 inputs; run explicitly in --release"]
    fn exhaustive_bit_identical_to_reference() {
        let threads = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4) as u64;
        let span = (1u64 << 32).div_ceil(threads);
        let started = std::time::Instant::now();

        let mut workers = std::vec::Vec::new();
        for t in 0..threads {
            workers.push(std::thread::spawn(move || {
                let end = ((t + 1) * span).min(1u64 << 32);
                let mut inputs = 0u64;
                let mut max_corrections = 0i64;
                let mut word = t * span;
                while word < end {
                    let corrections = check_against_reference(word as u32 as i32);
                    if corrections > max_corrections {
                        max_corrections = corrections;
                    }
                    inputs += 1;
                    word += 1;
                }
                (inputs, max_corrections)
            }));
        }

        let mut inputs = 0u64;
        let mut max_corrections = 0i64;
        for worker in workers {
            let (n, c) = worker.join().expect("proof worker panicked");
            inputs += n;
            max_corrections = max_corrections.max(c);
        }

        assert_eq!(inputs, 1u64 << 32, "proof did not cover every i32");
        std::println!(
            "exhaustive_bit_identical_to_reference: {inputs} inputs over {threads} threads, \
             max {max_corrections} correction step(s), {:?}",
            started.elapsed()
        );
    }

    /// Assert the shipped function agrees with the oracle for `x`, and report
    /// how many correction steps the candidate needed.
    fn check_against_reference(x: i32) -> i64 {
        let want = fsqrt_q32_reference(x);
        let got = __lp_lpir_fsqrt_q32(x);
        assert_eq!(got, want, "fsqrt_q32({x}): got {got}, reference {want}");
        if x <= 0 {
            return 0;
        }
        let corrections = i64::from(want) - i64::from(fsqrt_candidate(x as u32));
        assert!(
            corrections >= 0,
            "fsqrt_q32({x}): candidate overshot the exact root by {corrections}"
        );
        corrections
    }

    /// The original implementation, kept as the oracle: `u64::isqrt` of the
    /// input scaled by 2^16.
    fn fsqrt_q32_reference(x: i32) -> i32 {
        if x <= 0 {
            return 0;
        }
        let x_scaled = (x as u64) << 16;
        x_scaled.isqrt() as i32
    }

    /// A 64-bit LCG (Knuth's MMIX multiplier), used only to spread the sampled
    /// inputs; the low bits are never used as inputs.
    fn next_lcg(state: u64) -> u64 {
        state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407)
    }

    /// Convert float to fixed16x16 with saturation
    fn float_to_fixed(f: f32) -> i32 {
        const SCALE: f32 = 65536.0;
        const MAX_FLOAT: f32 = 0x7FFF_FFFF as f32 / SCALE;
        const MIN_FLOAT: f32 = i32::MIN as f32 / SCALE;

        if f > MAX_FLOAT {
            0x7FFF_FFFF
        } else if f < MIN_FLOAT {
            i32::MIN
        } else {
            (f * SCALE).round() as i32
        }
    }

    /// Convert fixed16x16 to float
    fn fixed_to_float(fixed: i32) -> f32 {
        fixed as f32 / 65536.0
    }
}
