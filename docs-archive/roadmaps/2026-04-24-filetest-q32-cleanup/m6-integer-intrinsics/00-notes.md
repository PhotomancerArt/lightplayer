# M6 Integer Intrinsics — Notes

## Goal

Implement or repair remaining intended integer builtin behavior on q32
targets.

## Current Findings

- Remaining integer GLSL builtins appear through Naga as
  `Expression::Math`, while `lps-frontend` currently lowers a subset
  and falls back to unsupported for many integer math variants.
- LPIR has useful integer/bitwise primitives (`Iand`, `Ior`, `Ixor`,
  shifts, immediates, `Select`), but no obvious dedicated `popcnt`,
  `clz`, or `mulh` op. M6 may need IR sequences or small imported
  helper functions for some builtins.
- `integer-bitcount.glsl` may be fully or partly handled by M2 if the
  issue is expectation/printer mismatch; re-run after M2 before coding.
- `common-intbitstofloat.glsl` needs row splitting: IEEE reinterpret /
  NaN / Inf behavior stays `@unsupported`; decimal literal-width bugs
  are parser/const-eval work and belong here.
- `roundEven` is intentionally owned by M2 despite being listed in
  `broken.md` Section E.
- Wide integer operations must remain suitable for no_std RV32.

## Questions For User

- Confirm `roundEven` is owned by M2 and should not be re-litigated in
  M6.
- For `imulExtended` / `umulExtended`, is the target bit-identical GLSL
  32x32 -> 64 behavior split into high/low outputs? **Answered:** Yes.
- Is `jit.q32` first-class for M6 marker removal, or is CI/product
  priority wasm + rv32 with jit handled opportunistically? **Answered:**
  No jit. `jit.q32` is deprecated; validate wasm, rv32c, and rv32n.

## Implementation Notes

- Keep real IEEE reinterpret behavior under `@unsupported`.
- Wide arithmetic must remain viable for no_std RV32.
- Re-run the integer builtin files after M2 and before implementation
  to avoid fixing rows that were expectation-only.
- Map each failing Naga `MathFunction` variant to either an LPIR
  sequence or a helper/import strategy before coding.

## Validation

- Targeted integer builtin filetests.
- Key files:
  `builtins/integer-bitfieldextract.glsl`,
  `builtins/integer-bitfieldinsert.glsl`,
  `builtins/integer-imulextended.glsl`,
  `builtins/integer-umulextended.glsl`,
  `builtins/integer-uaddcarry.glsl`,
  `builtins/integer-usubborrow.glsl`,
  `builtins/integer-findmsb.glsl`,
  `builtins/integer-bitcount.glsl`, and the literal-width rows of
  `builtins/common-intbitstofloat.glsl`.
- Final `just test-filetests`.
