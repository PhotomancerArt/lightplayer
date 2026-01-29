# Phase 5: Update Test Files

## Description

Update all test files to use the new overloaded function names instead of the old numbered names.

## Implementation Steps

1. Update `lp_hash.glsl`:
   - Change `lpfx_hash1(x, seed)` → `lpfx_hash(x, seed)`
   - Change `lpfx_hash2(x, y, seed)` → `lpfx_hash(uvec2(x, y), seed)`
   - Change `lpfx_hash3(x, y, z, seed)` → `lpfx_hash(uvec3(x, y, z), seed)`

2. Update `lp_simplex1.glsl`:
   - Change `lpfx_snoise1(x, seed)` → `lpfx_snoise(x, seed)`

3. Update `lp_simplex2.glsl`:
   - Change `lpfx_snoise2(p, seed)` → `lpfx_snoise(p, seed)`

4. Update `lp_simplex3.glsl`:
   - Change `lpfx_snoise3(p, seed)` → `lpfx_snoise(p, seed)`

5. Check for any other test files that reference the old function names

6. Run the filetests to verify everything works:
   ```bash
   scripts/glsl-filetests.sh
   ```

## Success Criteria

- All test files updated to use new function names
- Hash tests use `uvec2`/`uvec3` types where appropriate
- All filetests pass
- No references to old function names remain

## Notes

- The hash function tests will need to be updated to use `uvec2`/`uvec3` constructors
- Make sure the test expectations remain the same (only the function names change)
