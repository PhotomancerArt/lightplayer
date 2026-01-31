# Phase 3: Regenerate Builtin Registry

## Description

Run the builtin generator to automatically discover the new `__lp_*` functions and add them to the
`BuiltinId` enum in the registry.

## Implementation

### Run Builtin Generator

Execute the builtin generator:

```bash
cargo run --bin lp-glsl-builtins-gen-app --manifest-path lp-glsl/lp-glsl-builtins-gen-app/Cargo.toml
```

Or use the build script:

```bash
scripts/build-builtins.sh
```

### Expected Changes

The generator will:

1. Scan `lp-glsl-builtins/src/builtins/q32/` for new functions
2. Detect `__lpfx_hash_1`, `__lpfx_hash_2`, `__lpfx_hash_3`, `__lpfx_snoise1`, `__lpfx_snoise2`,
   `__lpfx_snoise3`
3. Add enum variants to `BuiltinId`: `LpHash1`, `LpHash2`, `LpHash3`, `LpSimplex1`, `LpSimplex2`,
   `LpSimplex3`
4. Generate `name()` method returning symbol names
5. Generate `signature()` method with correct parameter types
6. Update `mod.rs` exports
7. Update registry function pointer mappings

### Verify Generated Code

Check that `backend/builtins/registry.rs` includes:

- New enum variants in `BuiltinId`
- `name()` implementations returning `"__lpfx_hash_1"`, etc.
- `signature()` implementations with correct parameter counts and types
- Function pointer mappings in `get_function_pointer()`

## Success Criteria

- Builtin generator runs successfully
- All new functions appear in `BuiltinId` enum
- Signatures are correct (u32/i32 parameters, correct return types)
- `mod.rs` includes exports for new functions
- Code compiles after regeneration
- Code formatted with `cargo +nightly fmt`

## Notes

- The builtin generator is auto-generated code - don't edit manually
- If generator fails, check function signatures match expected format
- Ensure functions have `#[unsafe(no_mangle)]` attribute
