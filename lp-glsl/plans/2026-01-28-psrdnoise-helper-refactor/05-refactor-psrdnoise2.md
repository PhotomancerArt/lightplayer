# Phase 5: Refactor psrdnoise2_q32.rs to use helpers

## Description

Refactor `psrdnoise2_q32.rs` to use the new vector helper methods instead of manually expanded component operations. This will make the code more compact and closer to the original GLSL structure.

## Implementation

Update `builtins/lpfx/generative/psrdnoise/psrdnoise2_q32.rs`:

1. Replace manual component operations with vector operations:
   - `x.x - v0_x, x.y - v0_y` → `x - v0`
   - `floor(uv_x), floor(uv_y)` → `uv.floor()`
   - `fract(uv_x), fract(uv_y)` → `uv.fract()`
   - `step(f0_y, f0_x)` → `f0.step(f0.yx())` or similar

2. Use Vec2Q32 operations throughout:
   - Create vectors from components where needed
   - Use vector arithmetic instead of component-wise operations
   - Use helper methods (floor, fract, step, min, max, mod) instead of manual implementations

3. Maintain exact same functionality:
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
