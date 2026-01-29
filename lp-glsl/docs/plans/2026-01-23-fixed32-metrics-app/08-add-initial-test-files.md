# Phase 8: Add Initial Test GLSL Files

## Description

Add initial test GLSL files to the `glsl/` directory. Start with basic math operator tests and extract the complex shader from `tests.rs`.

## Implementation

- Extract the large shader from `lp-glsl/crates/lp-glsl-compiler/src/tests.rs` (the `analyze_shader_clif_size` test)
- Create `glsl/test-perlin.glsl` with the extracted shader
- Create basic math operator test files:
  - `glsl/test-add.glsl` - Simple addition operations
  - `glsl/test-sub.glsl` - Simple subtraction operations
  - `glsl/test-mul.glsl` - Simple multiplication operations
  - `glsl/test-div.glsl` - Simple division operations
- Each test should have a `main()` function that exercises the operations
- Keep tests simple but representative

## Success Criteria

- Test GLSL files are created in `glsl/` directory
- Files compile successfully
- Tests exercise various math operations
- At least one complex test (perlin) is included
