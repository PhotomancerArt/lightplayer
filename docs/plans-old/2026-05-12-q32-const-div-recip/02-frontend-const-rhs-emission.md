# Phase 02: Frontend Const RHS Emission

## Scope Of Phase

Teach GLSL lowering to emit `FdivConstF32` when the RHS of float division is a compile-time constant visible to the frontend.

In scope:

- Add a frontend helper for extracting float constants from Naga expressions.
- Emit `FdivConstF32` for scalar and vector float division where RHS constants are known.
- Add frontend tests showing the new op appears in lowered LPIR.

Out of scope:

- Broad constant propagation.
- Rewriting dynamic uniform/slot divisors.
- Backend optimization.

## Code Organization Reminders

- Keep Naga-expression inspection helpers close to existing frontend utility patterns.
- Prefer a small helper with an explicit name such as `float_const_lanes`.
- Keep test helpers below test functions.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-shader/lps-frontend/src/lower_binary.rs`
- `lp-shader/lps-frontend/src/lower_expr.rs`
- `lp-shader/lps-frontend/src/naga_util.rs`
- existing frontend tests under `lp-shader/lps-frontend/tests/`

Implementation sketch:

- In `lower_binary_vec`, before lowering the RHS for `BinaryOperator::Divide` with float operands, attempt to extract RHS constant lanes.
- Supported initial cases:
  - `Literal::F32`
  - `Literal::F64` converted to `f32`
  - `Expression::Constant` whose initializer resolves to a supported literal
  - scalar RHS broadcast over vector LHS
- Nice-to-have if Naga structure is straightforward:
  - vector constructor constants like `vec3(2.0, 4.0, 8.0)`
  - const vector names
- If the helper cannot prove constants for all RHS lanes needed, use the existing dynamic path.

Expected lowering:

```glsl
float f(float x) { return x / 2.0; }
```

should include `FdivConstF32` rather than `FconstF32` + `Fdiv`.

```glsl
float f(float x, float y) { return x / y; }
```

should keep dynamic `Fdiv`.

## Validate

```sh
cargo fmt --all
cargo test -p lps-frontend
cargo test -p lpir
```
