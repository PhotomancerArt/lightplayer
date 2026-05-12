# Phase 03: Backend Fast Lowering

## Scope Of Phase

Lower `FdivConstF32` in backends, with native Q32 optimized for the fastest normal shader path.

In scope:

- Native Q32 lowering for nonzero constants as multiply by precomputed Q32 reciprocal.
- Safe fallback for constant zero.
- Conservative wasm support so host paths continue to compile.
- Unit tests for native lowering shape.

Out of scope:

- General dynamic inline reciprocal division.
- Helper-equivalent const reciprocal lowering.
- Full deletion of mode types.

## Code Organization Reminders

- Factor shared native Q32 constant conversion if existing code duplicates `FconstF32` conversion.
- Reuse the existing inline wrapping Q32 multiply emission sequence rather than copying it blindly in multiple branches.
- Keep backend tests near related lowering tests at the bottom of the file.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-shader/lpvm-native/src/lower.rs`
- `lp-shader/lpvm-wasm/src/emit/ops.rs`
- `lp-shader/lpvm-wasm/src/emit/q32.rs`
- `lp-shader/lpvm-native/src/compile.rs` if tests need compile-level coverage

Native Q32 lowering:

- For `rhs != 0.0`:
  - compute `recip = q32(1.0 / rhs)` at compile time
  - materialize `recip` with `VInst::IConst32`
  - emit `dst = wrapping_q32_mul(lhs, recip)`
- For `rhs == 0.0`:
  - materialize zero into a temp
  - reuse the existing dynamic `Fdiv` helper path so div-zero behavior does not become accidental
- The optimized path should not branch on `Q32Options` unless keeping compatibility demands it. The normal path is fast.

Important semantic note:

- This phase intentionally does not preserve `__lp_lpir_fdiv_recip_q32(lhs, rhs_const)` bit-for-bit.
- The purpose is speed: ordinary shader math accepts approximation and edge divergence.

Native lowering tests should assert:

- nonzero const lowers to an `IConst32` reciprocal plus the inline multiply sequence, with no helper call
- zero const still lowers to helper/fallback behavior
- dynamic `Fdiv` remains unchanged unless deliberately touched

Wasm lowering:

- Minimum acceptable implementation: materialize the constant and use existing `Fdiv` emission.
- Preferred if straightforward: mirror native fast semantics for Q32 by multiplying by precomputed Q32 reciprocal.

## Validate

```sh
cargo fmt --all
cargo test -p lpvm-native
cargo test -p lpvm-wasm
```
