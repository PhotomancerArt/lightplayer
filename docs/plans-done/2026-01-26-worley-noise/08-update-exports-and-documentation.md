# Phase 8: Update Exports and Documentation

## Description

Update module exports to make Worley functions accessible, and ensure all documentation is complete
and consistent.

## Implementation

### Files to Update

1. **`lp-glsl-builtins/src/builtins/lpfx/worley/mod.rs`**
    - Add exports for all four Worley functions:
        - `pub mod worley2_q32;`
        - `pub mod worley2_value_q32;`
        - `pub mod worley3_q32;`
        - `pub mod worley3_value_q32;`

2. **Verify documentation**
    - Ensure all functions have proper doc comments
    - Verify GLSL usage examples in doc comments
    - Check that parameter and return value descriptions are accurate
    - Ensure references to noise-rs are included

### Documentation Requirements

Each function should have:

- Module-level documentation explaining Worley noise
- Function-level documentation with:
    - Description of what the function does
    - GLSL usage example
    - Parameter descriptions
    - Return value description (range [-1, 1])
    - Reference to noise-rs implementation

## Success Criteria

- All Worley functions are exported from the worley module
- All functions have complete documentation
- Documentation includes GLSL usage examples
- Documentation is consistent with Simplex noise documentation style
- Code formatted with `cargo +nightly fmt`

## Notes

- Follow the same documentation style as Simplex noise functions
- Include references to noise-rs and lygia where appropriate
- Ensure all doc comments are properly formatted
- Verify that examples compile (if using `# Examples` sections)
