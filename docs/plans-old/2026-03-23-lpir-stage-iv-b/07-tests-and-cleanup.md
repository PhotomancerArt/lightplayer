# Phase 7: Tests and Cleanup

## Scope

Run the full filetest suite, fix remaining failures, add targeted tests
for edge cases, clean up dead code and warnings.

## Implementation Details

### Run filetests

```
cargo test -p lps-frontend
cargo test -p lps-filetests
```

Triage failures:

- **Expected**: dynamic vector access tests tagged
  `@unimplemented(backend=wasm)` — these should error gracefully.
- **Regressions**: any previously-passing scalar test that now fails
  indicates a bug in the infrastructure refactoring (phase 1).
- **New passes**: vector/matrix tests that previously errored should now
  pass.

### Interpreter validation

Run GLSL → LPIR → interpret for key vector programs:

```
cargo test -p lps-frontend --test lower_interp
```

Add or verify tests for:

- `vec2`/`vec3`/`vec4` construction and component access
- Swizzle (`v.xzy`, `v.xx`)
- Vector arithmetic (`+`, `-`, `*`, `/`)
- Scalar broadcast (`vec3 * float`)
- `dot`, `cross`, `length`, `normalize`
- `mix` with vector and scalar t argument
- `mat3 * vec3`, `mat4 * vec4`
- Matrix transpose, determinant, inverse (for small matrices)
- LPFX call with vector out-parameter

### Cleanup

- Remove any leftover `naga_type_to_ir_type` calls that should now use
  `naga_type_to_ir_types` (or keep as convenience wrapper for known-scalar
  contexts).
- Remove `#[allow(dead_code)]` annotations that are no longer needed.
- Fix any new warnings from the SmallVec import or changed signatures.
- Ensure `cargo +nightly fmt` is clean.
- Ensure `cargo clippy` passes.

### Documentation

Update doc comments on changed functions to reflect vector support:

- `lower_expr.rs` header: remove "(scalar subset)" from module doc.
- `lower_stmt.rs` header: update if needed.
- `lower_math.rs` header: update to mention vector math.
- `lower_lpfx.rs` header: remove "(scalar subset)" from module doc.

## Validate

```
cargo test -p lps-frontend
cargo test -p lps-filetests
cargo +nightly fmt -- --check
cargo clippy --workspace
```

All tests pass. No warnings. Code formatted.
