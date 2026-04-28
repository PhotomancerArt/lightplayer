# M2 q32 Parity and Quick Wins — Notes

## Goal

Fix q32 parity drift, harness-only gaps, wrong expectations, and small
numeric/test cleanup before larger subsystem work.

## Current Findings

- Wasm q32 conversion work likely touches
  `lp-shader/lpvm-wasm/src/emit/q32.rs` and
  `lp-shader/lpvm-wasm/src/emit/ops.rs`, with cross-checks against
  rv32/cranelift paths and q32 builtins.
  **Implemented:** `lpvm-wasm` Q32 `FtoiSatS` / `FtoiSatU` now uses the
  existing trunc-toward-zero / negative-to-zero helpers instead of
  inline shifts.
- Behavioral filetests:
  - `scalar/int/from-float.glsl`
  - `scalar/uint/from-float.glsl`
  - `vec/uvec2/from-mixed.glsl`
  - `vec/uvec3/from-mixed.glsl`
  - `vec/uvec4/from-mixed.glsl`
  - `scalar/float/from-uint.glsl` if still failing on wasm
- `function/declare-prototype.glsl` is a harness issue: vector run
  args like `vec4(1.0)` currently fail parsing because
  `lps-filetests/src/util/value_parse.rs` requires exact component
  count rather than GLSL-style constructor splat.
- Suspected wrong expectation / printer rows:
  - `function/param-default-in.glsl`
  - `builtins/matrix-determinant.glsl`
  - `builtins/integer-bitcount.glsl`
- `builtins/common-roundeven.glsl` belongs here as q32 numeric behavior
  rather than in M6 integer intrinsics.
- `function/call-order.glsl` should only stay in M2 if investigation
  shows a small runtime parity / evaluation-order fix.
  **Investigation result:** defer from M2. The remaining rv32n failure
  is an `InvalidMemoryAccess` tied to native calls/globals/stack-frame
  behavior, not a quick q32 cast or expectation fix.

## Questions For User

- For filetest value parsing, should `vec4(1.0)` be supported as a
  GLSL-correct splat in the harness, or should the filetest be changed
  to explicit `vec4(1.0, 1.0, 1.0, 1.0)` arguments? **Answered:**
  support GLSL-style splat in the harness.
- If rv32, wasm, the reference `Q32` implementation, and
  `docs/design/q32.md` disagree after measurement, should the agent stop
  for a semantics decision or choose the most conservative product
  behavior and update the doc?
- Should `function/call-order.glsl` be included in M2 only after a
  minimal repro confirms it is a small fix? **Answered by
  investigation:** defer unless a later `DEBUG=1` trace reveals a
  tiny native emit/regalloc fix.

## Implementation Notes

- Reconcile `docs/design/q32.md`, the reference `Q32`
  implementation, and product backend behavior before changing numeric
  semantics.
- Suspected wrong expectations must be verified before editing tests.
- Remove `@broken` markers only after targeted runs pass.
- Update `docs/design/q32.md` in this milestone if the implementation
  fixes clarify q32 semantics.

## Validation

- Targeted filetests for Section B and quick-win files.
- Unit tests for `lps-filetests` if `value_parse` / parse helpers change:
  `cargo test -p lps-filetests`.
- Final `just test-filetests`.
