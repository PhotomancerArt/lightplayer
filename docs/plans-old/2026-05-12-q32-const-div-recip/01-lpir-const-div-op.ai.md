# Phase 01: LPIR Const-Div Op

## Scope Of Phase

Add a semantic LPIR operation for float division by a compile-time constant.

In scope:

- Add `LpirOp::FdivConstF32 { dst, lhs, rhs }`.
- Update LPIR formatting, parsing, validation, def/use metadata, and roundtrip tests.
- Add focused LPIR unit tests for the new op.

Out of scope:

- Frontend emission.
- Native/wasm optimized lowering.
- Removing old Q32 math modes.

## Code Organization Reminders

- Prefer granular helpers when the same op metadata logic appears in multiple places.
- Keep tests at the bottom of Rust files/modules.
- Put temporary compatibility code behind clear TODOs only if follow-up removal is already planned.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-shader/lpir/src/lpir_op.rs`
- `lp-shader/lpir/src/format.rs`
- `lp-shader/lpir/src/parse.rs`
- `lp-shader/lpir/src/validate.rs`
- `lp-shader/lpir/src/tests/all_ops_roundtrip.rs`
- `lp-shader/lpir/src/tests/validate.rs`
- `lp-shader/lpir/src/tests/interp.rs` if interpreter coverage is appropriate

Expected behavior:

- The op represents semantic float division by an immediate `f32`.
- `def_vreg()` returns `dst`.
- Use metadata includes only `lhs`.
- Formatting should be stable and search-friendly, for example:

```text
v3:f32 = fdiv_const.f32 v2, 2.0
```

- Parser should accept the chosen format and reject malformed immediates.
- Validator should ensure `dst` and `lhs` are `IrType::F32`.

Do not encode Q32 reciprocal behavior in LPIR. This op is semantic.

## Validate

```sh
cargo fmt --all
cargo test -p lpir
```
