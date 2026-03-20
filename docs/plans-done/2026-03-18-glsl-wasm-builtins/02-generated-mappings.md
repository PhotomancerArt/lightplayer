# Phase 2: Generated GLSL → `BuiltinId` mapping

## Scope of phase

- Extend **`lp-glsl-builtins-gen-app`** to emit **`lp-glsl-builtin-ids/src/glsl_builtin_mapping.rs`**:
  - `glsl_q32_math_builtin_id(name, ast_arg_count)` — standard math (`sin`, `atan` + 2 args → `LpQ32Atan2`, `roundEven`, …).
  - `glsl_lpfx_q32_builtin_id(name, params: &[GlslParamKind])` — LPFX overloads from the same metadata as `lpfx_fns.rs`.
  - `GlslParamKind` — mirrors GLSL parameter types for LPFX resolution.
- **`lib.rs`** (generated) adds `mod glsl_builtin_mapping` and re-exports the three items above.
- Ensure regeneration is wired to **`scripts/build-builtins.sh`** so new builtins/LPFX entries do not require hand-editing the compiler.
- Optionally emit **wasmtime linker registration stubs** or a data table (name → signature kind) consumed by `lp-glsl-filetests` — only if it reduces duplication; otherwise defer to phase 5 with a small hand-written table driven by `BuiltinId::all()`.

## Code organization reminders

- Generated files clearly marked “do not edit”; single generator entry point.
- Tests for the mapping live next to `lp-glsl-builtin-ids` or the generator.

## Implementation details

- Source of truth: existing builtin metadata already used for `map_testcase_to_builtin` / `BuiltinId` — extend the generator rather than duplicating tables.
- If GLSL name resolution needs more than `arg_count` (overloads), align with frontend `check_builtin_call` expectations and document any limitations in `00-design.md`.

## Validate

```bash
./scripts/build-builtins.sh   # or minimal generator run documented in crate README
cd lp-glsl && cargo test -p lp-glsl-builtin-ids
cd lp-glsl && cargo test -p lp-glsl-builtins-gen-app  # if tests exist
cargo +nightly fmt
```
