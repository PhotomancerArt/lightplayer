# Phase 2 — `fdiv_recip_q32` helper (port from git)

## Scope of phase

Add a new `extern "C"` helper `__lp_lpir_fdiv_recip_q32` that implements
fast Q32 division via reciprocal multiplication. Algorithm is ported from
the deleted `lp-glsl/.../div_recip.rs` (commit `1daa516^`), with an added
explicit `divisor == 0` saturation guard to match the existing
`__lp_lpir_fdiv_q32` semantic.

Phase 3 (`lpvm-native` dispatch) will sym_call this helper when
`Q32Options::div == DivMode::Reciprocal`. Phase 4 (`lpvm-wasm`) inlines
the same algorithm in wasm — but for **wasm the helper is not called**;
phase 4 ports the logic into `emit_q32_fdiv_recip` directly.

**Out of scope:**

- Wiring into `lpvm-native` (phase 3).
- Wasm inline expansion (phase 4).
- Any change to the existing `__lp_lpir_fdiv_q32` saturating helper.

## Code Organization Reminders

- One concept per file: this helper lives in its own module file.
- Tests live at the bottom of the file.
- Match the structure of existing siblings (`fadd_q32.rs`, `fsub_q32.rs`,
  `fmul_q32.rs`, `fdiv_q32.rs`).

## Sub-agent Reminders

- Do **not** commit. The plan commits at the end as a single unit.
- Do **not** expand scope.
- Do **not** suppress warnings — fix them.
- Do **not** modify the existing `fdiv_q32.rs` or other helpers.
- If something blocks completion, stop and report back.
- Report what changed and what was validated.

## Implementation Details

### Reference: original algorithm from git history

**Location:** `lp-glsl/crates/lp-glsl-compiler/src/backend/transform/fixed32/reference/div_recip.rs`
**Deleted in:** commit `1daa516e88a69ba6d1860b7927389d13198f384e`
**View command (don't need to re-read; reproduced below):**

```bash
git show 1daa516e88a69ba6d1860b7927389d13198f384e^:lp-glsl/crates/lp-glsl-compiler/src/backend/transform/fixed32/reference/div_recip.rs
```

Original (unsigned) algorithm:

```rust
fn fixed32_udiv(dividend: u32, divisor: u32) -> u32 {
    let recip = 0x8000_0000u32 / divisor;
    let quotient = (((dividend as u64) * (recip as u64) * 2u64) >> 16) as u32;
    quotient
}

fn fixed32_idiv(dividend: i32, divisor: i32) -> i32 {
    let result_sign = if (dividend ^ divisor) < 0 { -1 } else { 1 };
    (fixed32_udiv(dividend.abs() as u32, divisor.abs() as u32) as i32) * result_sign
}
```

The original did not handle `divisor == 0` (would panic on integer divide
by zero). We must add that guard to match the existing
`__lp_lpir_fdiv_q32` saturation policy.

### File: `lp-shader/lps-builtins/src/builtins/lpir/fdiv_recip_q32.rs`

Create new file matching the structure of `fdiv_q32.rs` (read it first to
match conventions: header comment, MAX/MIN constants, `#[unsafe(no_mangle)]
pub extern "C" fn ...`, tests below).

```rust
//! Fixed-point 16.16 division via reciprocal multiplication.
//!
//! Faster than [`__lp_lpir_fdiv_q32`] (one i32 udiv + 2 muls + shift +
//! sign fixup vs one i64 div), at the cost of small precision loss:
//! ~0.01% typical error, up to ~2-3% at edges (saturated dividends, very
//! small divisors).
//!
//! Selected when the shader opts into `Q32Options { div: Reciprocal, .. }`.
//! See `docs/plans-old/2026-04-18-q32-options-dispatch/00-design.md`.
//!
//! ## Algorithm
//!
//! Ported from `lp-glsl/.../div_recip.rs` (deleted in commit `1daa516`).
//! The `divisor == 0` guard is new — original would panic on integer
//! divide; we saturate instead, matching `__lp_lpir_fdiv_q32`.
//!
//! ```text
//! recip = 0x8000_0000 / |divisor|              (one i32 udiv, truncates)
//! quot  = (|dividend| * recip * 2) >> 16       (u64 multiply intermediate)
//! quot *= sign(dividend) ^ sign(divisor)
//! ```
//!
//! For `divisor == 0`: returns `0` for `0/0`, `MAX_FIXED` for positive/0,
//! `MIN_FIXED` for negative/0.

const MAX_FIXED: i32 = 0x7FFF_FFFF;
const MIN_FIXED: i32 = i32::MIN;

/// Q16.16 division by reciprocal multiplication.
///
/// See module docs for algorithm and precision notes.
#[unsafe(no_mangle)]
pub extern "C" fn __lp_lpir_fdiv_recip_q32(dividend: i32, divisor: i32) -> i32 {
    if divisor == 0 {
        if dividend == 0 {
            return 0;
        } else if dividend > 0 {
            return MAX_FIXED;
        } else {
            return MIN_FIXED;
        }
    }

    let result_sign = if (dividend ^ divisor) < 0 { -1i32 } else { 1i32 };

    let abs_dividend = dividend.unsigned_abs();
    let abs_divisor = divisor.unsigned_abs();

    let recip = 0x8000_0000u32 / abs_divisor;
    let quot = (((abs_dividend as u64) * (recip as u64) * 2u64) >> 16) as u32;

    (quot as i32).wrapping_mul(result_sign)
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use crate::util::test_helpers::{fixed_to_float, float_to_fixed};

    /// Tolerance used for "approximately equal" comparisons in tests.
    /// Reciprocal mul has documented ~0.01% typical error; we use a slightly
    /// looser bound to keep tests stable across platforms.
    const TOL: f32 = 0.001;

    fn check(dividend: f32, divisor: f32, expected: f32) {
        let d = float_to_fixed(dividend);
        let s = float_to_fixed(divisor);
        let r = fixed_to_float(__lp_lpir_fdiv_recip_q32(d, s));
        assert!(
            (r - expected).abs() < TOL,
            "fdiv_recip_q32({dividend}, {divisor}) = {r}, expected {expected}"
        );
    }

    #[test]
    fn basic_unsigned() {
        check(10.0, 2.0, 5.0);
        check(15.0, 3.0, 5.0);
        check(20.0, 2.0, 10.0);
        check(7.5, 1.0, 7.5);
        check(0.999, 0.998, 0.999 / 0.998);
    }

    #[test]
    fn basic_signed() {
        check(10.0, -2.0, -5.0);
        check(-10.0, 2.0, -5.0);
        check(-10.0, -2.0, 5.0);
        check(-7.5, 3.0, -2.5);
    }

    #[test]
    fn small_divisors() {
        check(1.0, 0.5, 2.0);
        check(0.25, 0.5, 0.5);
    }

    #[test]
    fn divide_by_zero_saturates() {
        // Match __lp_lpir_fdiv_q32 policy.
        assert_eq!(__lp_lpir_fdiv_recip_q32(0, 0), 0);
        assert_eq!(
            __lp_lpir_fdiv_recip_q32(float_to_fixed(1.0), 0),
            MAX_FIXED
        );
        assert_eq!(
            __lp_lpir_fdiv_recip_q32(float_to_fixed(-1.0), 0),
            MIN_FIXED
        );
    }

    #[test]
    fn matches_saturating_helper_within_tolerance() {
        // For "normal" cases (non-edge), the reciprocal helper should be
        // within ~0.01% of the saturating helper.
        let cases: &[(f32, f32)] = &[
            (10.0, 3.0),
            (1.5, 0.25),
            (-7.0, 2.5),
            (100.0, 7.0),
            (0.5, 0.125),
        ];
        for &(a, b) in cases {
            let af = float_to_fixed(a);
            let bf = float_to_fixed(b);
            let sat =
                fixed_to_float(crate::builtins::lpir::fdiv_q32::__lp_lpir_fdiv_q32(af, bf));
            let recip = fixed_to_float(__lp_lpir_fdiv_recip_q32(af, bf));
            // Within 0.1% relative or 0.001 absolute, whichever is larger.
            let tol = (sat.abs() * 0.001).max(0.001);
            assert!(
                (sat - recip).abs() < tol,
                "fdiv divergence at {a}/{b}: sat={sat}, recip={recip}"
            );
        }
    }
}
```

### Register the new module

File: `lp-shader/lps-builtins/src/builtins/lpir/mod.rs`

Add the `pub mod fdiv_recip_q32;` declaration (alphabetical with siblings).
Read the file first to see the existing pattern.

### Add to builtin registry / table

The lpvm-native and lpvm-cranelift backends look up builtins by name via
their respective registry mechanisms. Two known sites need to be checked:

1. **lpvm-cranelift**: `lp-shader/lpvm-cranelift/src/generated_builtin_abi.rs`
   contains the `BuiltinId` enum and lookup table. This is **generated** —
   inspect first to see if it's hand-edited or generated by a script.
   - If hand-edited: add a `LpLpirFdivRecipQ32` variant matching the
     pattern of `LpLpirFdivQ32`, and the corresponding lookup arm.
   - If generated: there will be a generator script (likely
     `scripts/build-builtins.sh` or similar in `lp-shader/lpvm-cranelift/`).
     Run it to regenerate. Check the file header for hints.

2. **lpvm-native `BuiltinTable`**: `lp-shader/lpvm-native/src/...` —
   search for existing references to `__lp_lpir_fdiv_q32` to find the
   registration site. Likely a `populate()` method that inserts each
   builtin by name. Add an analogous entry for `__lp_lpir_fdiv_recip_q32`.

3. **`ensure_builtins_referenced`**: search `lps-builtins/src/lib.rs` for
   `ensure_builtins_referenced` (used to keep symbols from being dead-code-
   eliminated when linking statically). Add a reference to the new function
   if the existing pattern requires it.

If you find that the cranelift `BuiltinId` registry is generated and the
generator is unrelated to this phase, **report back instead of running
the generator** — the generator may have its own build steps and we don't
want to conflate the two changes. In that case, leave the cranelift side
incomplete and document it for phase 6.

For lpvm-native, the lookup is by name (string) and goes through
`ModuleSymbols`, so the only required wiring is making sure the symbol
exists in `lps-builtins` and is exported. The native `BuiltinTable`
populate path does need an entry though — check the existing pattern.

### Verify the new symbol is exported

After adding, run:

```bash
cargo build -p lps-builtins
nm target/debug/liblps_builtins.a 2>/dev/null | grep fdiv_recip || \
  cargo nm -p lps-builtins | grep fdiv_recip || \
  echo "(skip nm verification)"
```

Visual confirmation that the helper compiles and is reachable. Don't fail
this phase if `nm` isn't available; the unit tests are the real validation.

## Validate

From workspace root:

```bash
cargo check -p lps-builtins
cargo test -p lps-builtins
cargo build --workspace
```

All `lps-builtins` tests pass, including the new
`fdiv_recip_q32::tests::*`. No warnings. Workspace builds.

## Definition of done

- New file `lp-shader/lps-builtins/src/builtins/lpir/fdiv_recip_q32.rs`
  with the helper, doc comment, and 5 tests (basic_unsigned, basic_signed,
  small_divisors, divide_by_zero_saturates,
  matches_saturating_helper_within_tolerance).
- `mod.rs` registers the new module.
- `BuiltinTable::populate` in lpvm-native registers
  `__lp_lpir_fdiv_recip_q32` (or equivalent registration mechanism).
- Cranelift `BuiltinId` registry: either updated (if hand-edited), or left
  alone with a note for phase 6 (if generated).
- `ensure_builtins_referenced` updated if the existing pattern requires it.
- All `lps-builtins` tests pass; workspace builds cleanly.
- No new warnings.
