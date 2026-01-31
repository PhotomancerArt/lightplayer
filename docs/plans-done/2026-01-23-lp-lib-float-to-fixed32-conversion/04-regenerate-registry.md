# Phase 4: Regenerate Builtin Registry

## Goal

Run the fixed generator to regenerate the builtin registry with correct `BuiltinId` variant names
that match `LpLibFn::builtin_id()` expectations.

## Tasks

### 4.1 Run Builtin Generator

Run the generator:

```bash
cargo run --bin lp-glsl-builtins-gen-app --manifest-path lp-glsl/lp-glsl-builtins-gen-app/Cargo.toml
```

Or use the build script:

```bash
scripts/build-builtins.sh
```

### 4.2 Verify Registry Output

Check `lp-glsl/lp-glsl-compiler/src/backend/builtins/registry.rs`:

- Should have `LpSimplex1`, `LpSimplex2`, `LpSimplex3` variants (not `Q32LpSimplex*`)
- `BuiltinId::LpSimplex3.name()` should return `"__lp_q32_lpfx_snoise3"`
- `BuiltinId::LpHash1.name()` should return `"__lpfx_hash_1"` (unchanged)

### 4.3 Verify Function Pointer Mapping

Check `get_function_pointer()`:

- `BuiltinId::LpSimplex3` should map to `q32::__lp_q32_lpfx_snoise3`
- Hash functions should map correctly to `q32::__lpfx_hash_*` or `shared::__lpfx_hash_*`

### 4.4 Fix Any Compilation Errors

If registry changes cause compilation errors:

- Fix `lp_lib_fns.rs` if `BuiltinId` variant names changed
- Update any other code that references the old variant names

## Success Criteria

- Registry has correct `BuiltinId` variant names (`LpSimplex3`, not `Q32LpSimplex3`)
- `BuiltinId::LpSimplex3.name()` returns `"__lp_q32_lpfx_snoise3"`
- All code compiles without errors
- Code formatted with `cargo +nightly fmt`

## Code Organization

- Place helper utility functions **at the bottom** of files
- Place more abstract things, entry points, and tests **first**
- Keep related functionality grouped together

## Formatting

- Run `cargo +nightly fmt` on all changes before committing
- Ensure consistent formatting across modified files

## Language and Tone

- Keep language professional and restrained
- Avoid overly optimistic language like "comprehensive", "fully production ready", "complete
  solution"
- Avoid emoticons
- Code is never done, never perfect, never fully ready, never fully complete
- Use measured, factual descriptions of what was implemented
