# Phase 6: Refactor psrdnoise3_q32.rs to use helpers

## Description

Refactor `psrdnoise3_q32.rs` to use the new vector helper methods instead of manually expanded component operations. This will make the code more compact and closer to the original GLSL structure.

## Implementation

Update `builtins/lpfx/generative/psrdnoise/psrdnoise3_q32.rs`:

1. Replace manual component operations with vector operations:
   - `x.x - v0_x, x.y - v0_y, x.z - v0_z` → `x - v0`
   - `floor(uvw_x), floor(uvw_y), floor(uvw_z)` → `uvw.floor()`
   - `fract(uvw_x), fract(uvw_y), fract(uvw_z)` → `uvw.fract()`
   - `step(f0.xyx, f0.yzz)` → `f0.xyx().step(f0.yzz())` or `f0.step(f0.yzx())`

2. Use Vec3Q32 and Vec4Q32 operations throughout:
   - Create vectors from components where needed
   - Use vector arithmetic instead of component-wise operations
   - Use helper methods (floor, fract, step, min, max, mod) instead of manual implementations
   - Use Vec4Q32 for hash computations (`vec4(i0.z, i1.z, i2.z, i3.z)`)
   - Use `from_vec3_scalar()` for creating Vec4 from Vec3 + scalar

3. Replace manual trigonometric operations:
   - `sin(theta_x), sin(theta_y), ...` → `theta.sin()` (for Vec4Q32)
   - `cos(theta_x), cos(theta_y), ...` → `theta.cos()` (for Vec4Q32)
   - `sqrt(sz_prime_x), ...` → `sz_prime.sqrt()` (for Vec4Q32)

4. Maintain exact same functionality:
   - All calculations must produce identical results
   - No changes to algorithm or constants
   - Only refactoring for readability/maintainability

## Success Criteria

- Code uses vector helpers throughout
- Estimated 30-40% code reduction
- All existing tests pass (no functional changes)
- Code structure matches GLSL more closely
- Code compiles without errors or warnings

## Code Organization

- Place helper utility functions at the bottom of files
- Place more abstract things, entry points, and tests first
- Keep related functionality grouped together

## Formatting

- Run `cargo +nightly fmt` on all changes before committing
- Ensure consistent formatting across modified files

## Language and Tone

- Keep language professional and restrained
- Avoid overly optimistic language like "comprehensive", "fully production ready", "complete solution"
- Avoid emoticons
- Code is never done, never perfect, never fully ready, never fully complete
- Use measured, factual descriptions of what was implemented
