# Phase 6: Regenerate Builtin Registry

## Description

Run the builtin generator to automatically register the new Worley noise functions in the builtin
system. The generator scans `lpfx/worley/` directory and adds functions to the `BuiltinId` enum.

## Implementation

### Steps

1. Run the builtin generator: `cargo run --bin lp-glsl-builtin-gen-app` (or appropriate command)
2. Verify that new functions are registered in `BuiltinId` enum
3. Verify that function signatures are correctly parsed
4. Ensure all functions are accessible via the builtin system

### Expected Changes

- `BuiltinId` enum will have new variants: `LpWorley2`, `LpWorley2Value`, `LpWorley3`,
  `LpWorley3Value`
- Function signatures will be registered correctly
- Functions will be callable from GLSL shaders

## Success Criteria

- Builtin generator runs without errors
- All four Worley functions are registered in `BuiltinId` enum
- Function signatures are correctly parsed from `#[lpfx_impl_macro::lpfx_impl]` attributes
- Code compiles without errors
- Code formatted with `cargo +nightly fmt`

## Notes

- The builtin generator should automatically discover functions with `#[lpfx_impl_macro::lpfx_impl]`
  attributes
- If generator needs updates, those should be done in this phase
- Verify that the generator correctly handles the `_value` suffix in function names
