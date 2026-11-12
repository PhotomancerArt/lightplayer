# Milestone III: Bvec lowering gaps

## Goal

Remaining bvec-specific filetest failures pass on `jit.q32`: casts, `mix(bvec)`, and any
stragglers from Milestones I–II.

## Suggested plan name

`lpir-parity-milestone-iii`

## Scope

**In scope:**

- **Bvec → float/int/uint casts** (`vec2(bvec2(…))`): fails with `assignment component count 1
  vs 2`. The lowering produces a scalar where the caller expects a vector. Fix the `As` /
  `Compose` path in `lower_expr.rs` for bool → numeric vector conversions.
- **`mix(vec, vec, bvec)`**: Naga reports `Ambiguous best function for 'mix'`. This is a Naga
  frontend limitation (it doesn't resolve the GLSL `genType mix(genType, genType, genBType)`
  overload). Mark `@unimplemented(reason="Naga frontend limitation")`.
- **`while (bool j = expr)` condition declaration**: Naga can't parse this GLSL syntax. Mark
  `@unimplemented(reason="Naga frontend limitation")`.
- **Forward-declare / param-unnamed files** that fail because the file contains array/matrix
  declarations in other functions: triage individually. If the **tested** function is clean but
  another declaration poisons the compile, restructure the lowering to be lazy (only lower
  functions that are actually called) or split the test file.
- **`const/builtin/extended.glsl`** (1/3 failing case): `round(2.5)` Q32 tie-breaking. Mark the
  specific case `@unsupported(float_mode=q32, reason="Q32 round tie differs from IEEE")` or fix
  Q32 round if the correct behavior is well-defined.

**Out of scope:**

- Array types (Milestone IV).
- Matrix invoke (Milestone V).

## Key decisions

- Naga parse/overload limitations get `@unimplemented(reason="Naga frontend limitation")` rather
  than fork modifications.
- Forward-declare poisoning: prefer splitting the test file over making the compiler lazy (lazy
  lowering is a larger change with wider implications).

## Deliverables

- Fix bvec cast path in `lower_expr.rs`.
- Annotation updates on ~4 files (Naga limitations).
- Triage and fix/split ~3 forward-declare/param-unnamed/const files.
- ~6 filetest files resolved (pass or annotated).

## Dependencies

Milestones I and II — bvec casts interact with relational results and pointer-based stores.

## Estimated scope

Small. Mostly targeted fixes and annotation decisions; the cast fix is likely <30 lines.
