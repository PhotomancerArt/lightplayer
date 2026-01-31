# Phase 5: Matrix Complex Functions

## Goal

Implement matrix builtins for complex operations: determinant and inverse.

## Functions to Implement

- **determinant** (2x2, 3x3, 4x4) - tests: `builtins/matrix-determinant.glsl`
- **inverse** (2x2, 3x3, 4x4) - tests: `builtins/matrix-inverse.glsl`

## Implementation Details

### determinant

- 2x2: `det = a*d - b*c`
- 3x3: Use cofactor expansion
- 4x4: Use cofactor expansion or block decomposition
- Operates on q32 values (matrix elements are q32)
- File: `lightplayer/crates/lp-glsl-builtins/src/q32/determinant.rs` (new file)
- Note: These functions take multiple arguments (4 for 2x2, 9 for 3x3, 16 for 4x4)

### inverse

- 2x2: `inv = (1/det) * [d -b; -c a]`
- 3x3: Use adjugate method: `inv = (1/det) * adj(M)`
- 4x4: Use adjugate method
- Requires determinant calculation
- File: `lightplayer/crates/lp-glsl-builtins/src/q32/inverse.rs` (new file)
- Note: These functions take multiple arguments and return multiple values

## Considerations

- Matrix functions operate on multiple q32 values
- May need to handle as separate functions per matrix size (determinant_mat2, determinant_mat3,
  etc.)
- Or use a single function with size parameter
- Check how existing codegen handles matrices

## Files to Create/Modify

- `lightplayer/crates/lp-glsl-builtins/src/q32/determinant.rs` - new file
- `lightplayer/crates/lp-glsl-builtins/src/q32/inverse.rs` - new file
- May need to update builtin generator to handle multi-arg/multi-return functions
- Run builtin generator to update boilerplate

## Success Criteria

- Functions implemented following existing patterns
- Unit tests pass for all matrix sizes (2x2, 3x3, 4x4)
- Functions registered in builtin generator
- Code compiles without warnings
- Tests pass: `scripts/glsl-filetests.sh builtins/matrix-determinant.glsl`, etc.

