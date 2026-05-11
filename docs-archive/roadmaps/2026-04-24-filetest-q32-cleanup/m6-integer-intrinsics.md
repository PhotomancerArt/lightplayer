# Milestone 6: Integer Intrinsics

## Goal

Implement or repair the remaining intended integer builtin behavior on
q32 targets.

## Suggested plan location

Implementation plan:
`docs/roadmaps/2026-04-24-filetest-q32-cleanup/m6-integer-intrinsics/`

Use `/plan` to produce:
`docs/roadmaps/2026-04-24-filetest-q32-cleanup/m6-integer-intrinsics/00-notes.md`,
`00-design.md`, and numbered phase files.

## Scope

In scope:

- Implement scalar and vector `bitfieldExtract`.
- Implement scalar and vector `bitfieldInsert`.
- Implement `imulExtended` and `umulExtended` using 32-bit-compatible
  wide multiply / high-word behavior.
- Implement `uaddCarry` and `usubBorrow`.
- Fix `findMSB` edge cases for negative values and `0x8000_0000`.
- Fix the large decimal literal-width bug affecting the broken portion
  of `common-intbitstofloat.glsl`.
- Re-run `integer-bitcount.glsl` after M2; only handle real
  implementation bugs here if the M2 expectation/printer fix did not
  retire it.

Out of scope:

- Real IEEE `intBitsToFloat` / `floatBitsToInt` behavior, NaN/Inf
  reinterpretation, and infinite literals; those are `@unsupported`
  on q32.
- `roundEven`, which is q32 float numeric behavior and belongs to M2.
- Non-integer matrix/vector builtins.

## Key decisions

- Integer intrinsics are intended q32 functionality where they operate
  on integer payloads and do not require real IEEE f32.
- Wide operations must remain viable for the no_std RV32 product path.
- Literal-width fixes should be in the parser/const-eval path, not
  hidden inside one builtin's lowering.

## Deliverables

- Passing targeted integer builtin filetests for the scoped rows.
- Removed `@broken` markers for retired integer rows.
- Parser/const-eval test coverage for large decimal literal width where
  appropriate.
- `just test-filetests` passing at the milestone baseline.

## Dependencies

- Milestone 1 annotation baseline.
- Milestone 2 bitcount expectation/printer cleanup.

## Execution Strategy

**Option C — Full plan (`/plan`).**

> This milestone needs a full plan. I'll run the `/plan` process —
> question iteration, design, then phase files — and then `/implement`
> to dispatch. Agree?

Full plan: the integer rows span builtin lowering, vectorization,
literal parsing/const-eval, and RV32-compatible wide arithmetic.
