# Phase 4: Regenerate Builtin Registry

## Description

Regenerate the builtin registry to pick up the new GLSL function signatures. The codegen tool will automatically detect the updated signatures and generate multiple `LpfxFn` entries for each overloaded function name.

## Implementation Steps

1. Run the builtin generator:
   ```bash
   cargo run --bin lp-builtin-gen --manifest-path lp-glsl/apps/lp-builtin-gen/Cargo.toml
   ```
   
   Or use the build script:
   ```bash
   scripts/build-builtins.sh
   ```

2. Verify the generated `lpfx_fns.rs` file contains:
   - Multiple entries for `lpfx_hash` with different signatures (`uint`, `uvec2`, `uvec3`)
   - Multiple entries for `lpfx_snoise` with different signatures (`float`, `vec2`, `vec3`)
   - Multiple entries for `lpfx_worley` with different signatures (`vec2`, `vec3`)
   - Multiple entries for `lpfx_worley_value` with different signatures (`vec2`, `vec3`)

3. Check that old function names (`lpfx_hash1`, `lpfx_snoise1`, etc.) are no longer present

4. Verify the code compiles

## Success Criteria

- Builtin registry regenerated successfully
- New overloaded function names appear in `lpfx_fns.rs`
- Old numbered function names are removed
- Code compiles without errors
- No warnings about missing functions

## Notes

- The codegen tool should automatically handle overloads since we updated the GLSL signatures in the macro annotations
- If the tool doesn't pick up the changes, we may need to check the parsing logic
