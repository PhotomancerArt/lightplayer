# Phase 3: Implement GLSL Compilation and Transform Logic

## Description

Implement the core compilation logic that compiles GLSL files to CLIF IR, applies the q32 transform, and manages the before/after module states.

## Implementation

- Create `src/compiler.rs`
- Implement `compile_glsl()` function:
  - Takes GLSL source string
  - Uses `GlslCompiler` to compile to `GlModule`
  - Returns `GlModule` (before transform)
- Implement `apply_transform()` function:
  - Takes `GlModule` and `FixedPointFormat`
  - Creates `Q32Transform` with the format
  - Applies transform using `apply_transform()`
  - Returns transformed `GlModule`
- Handle errors and abort on failure

## Success Criteria

- GLSL compilation works correctly
- Q32 transform is applied successfully
- Errors are handled and reported clearly
- Code compiles without errors
