# Phase 3: Testing and Verification

## Goal

Add tests and verify the implementation works end-to-end.

## Tasks

### 3.1 Add Unit Tests

In `lp-glsl-builtins/src/q32/sin.rs`:

- Add tests using the test helper functions from phase 1
- Test cases:
    - Basic values: sin(0) = 0, sin(π/2) ≈ 1, sin(π) ≈ 0
    - Negative angles: sin(-π/2) ≈ -1
    - Range reduction: sin(2π) ≈ 0, sin(3π/2) ≈ -1
- Use ~2-3% tolerance for assertions

In `lp-glsl-builtins/src/q32/cos.rs`:

- Add tests using test helper functions
- Test cases:
    - Basic values: cos(0) = 1, cos(π/2) ≈ 0, cos(π) ≈ -1
    - Relationship to sin: cos(x) ≈ sin(x + π/2)
- Use ~2-3% tolerance

### 3.2 Verify GLSL Filetest

Run `scripts/glsl-filetests.sh builtins/trig-sin.glsl`:

- Should pass (or show progress if not fully passing yet)
- Debug any issues with transform conversion or implementation

### 3.3 Integration Test

Create a simple GLSL test that uses both sin and cos:

- Verify both functions work in the same shader
- Verify component-wise operation works (vec2, vec3, vec4)

## Success Criteria

- Unit tests pass for sin and cos
- `scripts/glsl-filetests.sh builtins/trig-sin.glsl` passes
- Integration test shows both functions work together
- All code compiles without warnings

