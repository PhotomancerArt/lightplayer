//! `code_walk` — fetch misses over a body of code larger than any plausible
//! instruction cache.
//!
//! Sixteen `#[inline(never)]` functions, each a straight line of
//! [`STEPS_PER_LEG`] xorshift-multiply steps, walked in order. The pair of
//! kernels around it (`code_walk/cold`, `code_walk/warm`) walks the chain
//! twice back to back inside one repetition: the first walk pays whatever a
//! fetch miss costs, the second shows what a hit costs, and the difference is
//! the quantity.
//!
//! ## The size, and the hypothesis it is sized against
//!
//! [`CODE_WALK_STEPS`] steps of roughly three instructions each is on the
//! order of **96 KiB of `.text`**, sized to exceed a **32 KiB** L1 cache by
//! about 3×. That 32 KiB is a *hypothesis*, not a citation: the ESP32-C6's
//! cache geometry — size, line length, ways — is not in this repository, and
//! esp-hal 1.1.1 and esp-metadata 0.4.0 carry no cache constant for the part
//! (`notes.md` F7). **Establishing the geometry is P3's job (OQ3)**; the two
//! sources named there are the TRM's Cache chapter and the ROM's `Cache_*`
//! writes into EXTMEM.
//!
//! So this kernel is deliberately over-sized rather than tuned. If the real
//! cache turns out to be larger than 32 KiB, the walk is still bigger than it
//! and the measurement stands; the one thing it cannot do is *locate* the
//! cache's edge, and nothing here claims to.
//!
//! The actual byte size is not asserted here — a compiled size is a fact
//! about a build, not about arithmetic — it is measured off the ELF and
//! recorded in the calibration report with its provenance.

/// Straight-line steps in one leg of the chain.
const STEPS_PER_LEG: u32 = 512;

/// Legs in the chain.
const LEGS: u32 = 16;

/// Total straight-line steps walked in one pass. Reported as the kernel's
/// `iters`; there is deliberately no `insns` figure, because a compiled step
/// is however many instructions the compiler chose.
pub const CODE_WALK_STEPS: u32 = STEPS_PER_LEG * LEGS;

/// Eight steps. Xorshift-then-multiply, because it does not compose: a chain
/// of affine steps (`a * k + c`) folds into one affine step and the walk would
/// vanish at `opt-level` anything.
macro_rules! step8 {
    ($a:ident, $k:expr) => {
        $a = ($a ^ ($a >> 3)).wrapping_mul($k);
        $a = ($a ^ ($a >> 5)).wrapping_mul($k);
        $a = ($a ^ ($a >> 7)).wrapping_mul($k);
        $a = ($a ^ ($a >> 11)).wrapping_mul($k);
        $a = ($a ^ ($a >> 3)).wrapping_mul($k);
        $a = ($a ^ ($a >> 5)).wrapping_mul($k);
        $a = ($a ^ ($a >> 7)).wrapping_mul($k);
        $a = ($a ^ ($a >> 11)).wrapping_mul($k);
    };
}

macro_rules! step64 {
    ($a:ident, $k:expr) => {
        step8!($a, $k);
        step8!($a, $k);
        step8!($a, $k);
        step8!($a, $k);
        step8!($a, $k);
        step8!($a, $k);
        step8!($a, $k);
        step8!($a, $k);
    };
}

/// One leg: 512 straight-line steps, never inlined, and given its own
/// multiplier so that identical-code folding cannot merge the sixteen legs
/// back into one — which would leave a walk of 6 KiB claiming to be 96.
macro_rules! leg {
    ($name:ident, $k:expr) => {
        #[inline(never)]
        fn $name(mut a: u32) -> u32 {
            step64!(a, $k);
            step64!(a, $k);
            step64!(a, $k);
            step64!(a, $k);
            step64!(a, $k);
            step64!(a, $k);
            step64!(a, $k);
            step64!(a, $k);
            a
        }
    };
}

leg!(leg00, 0x9E37_79B1);
leg!(leg01, 0x85EB_CA6B);
leg!(leg02, 0xC2B2_AE35);
leg!(leg03, 0x27D4_EB2F);
leg!(leg04, 0x1656_67B1);
leg!(leg05, 0x2545_F491);
leg!(leg06, 0x7FEB_352D);
leg!(leg07, 0x846C_A68B);
leg!(leg08, 0xCC9E_2D51);
leg!(leg09, 0x1B87_3593);
leg!(leg10, 0xE653_5C4D);
leg!(leg11, 0xAF25_1AF3);
leg!(leg12, 0xB55A_4F09);
leg!(leg13, 0x3C6E_F35F);
leg!(leg14, 0x0019_660D);
leg!(leg15, 0x6C07_8965);

/// Walk the chain once. `_steps` is the declared iteration count and is
/// ignored — the walk's length is the chain's, not a parameter, because a
/// runtime-variable walk would not be straight-line code.
pub fn walk_once(_steps: u32) -> u32 {
    let mut a = 0x1234_5678u32;
    a = leg00(a);
    a = leg01(a);
    a = leg02(a);
    a = leg03(a);
    a = leg04(a);
    a = leg05(a);
    a = leg06(a);
    a = leg07(a);
    a = leg08(a);
    a = leg09(a);
    a = leg10(a);
    a = leg11(a);
    a = leg12(a);
    a = leg13(a);
    a = leg14(a);
    leg15(a)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The walk is deterministic — the same accumulator every time, on any
    /// host and on the chip. That is what makes `acc` a structural field.
    #[test]
    fn the_walk_is_deterministic() {
        assert_eq!(walk_once(0), walk_once(CODE_WALK_STEPS));
    }

    /// Sixteen distinct multipliers, so nothing folds the legs together.
    #[test]
    fn the_legs_are_distinguishable() {
        let a = 0x1234_5678u32;
        let outs = [
            leg00(a),
            leg01(a),
            leg02(a),
            leg03(a),
            leg04(a),
            leg05(a),
            leg06(a),
            leg07(a),
            leg08(a),
            leg09(a),
            leg10(a),
            leg11(a),
            leg12(a),
            leg13(a),
            leg14(a),
            leg15(a),
        ];
        for (i, x) in outs.iter().enumerate() {
            for (j, y) in outs.iter().enumerate() {
                assert!(i == j || x != y, "legs {i} and {j} agree — they folded");
            }
        }
    }

    #[test]
    fn the_declared_step_count_matches_the_chain() {
        assert_eq!(CODE_WALK_STEPS, 8192);
    }
}
