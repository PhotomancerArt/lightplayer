# Phase 8: Add filetests

## Description

Add filetests for all new functions to verify they work correctly. Create test files in the filetests directory.

## Implementation

### File: `lp-glsl-filetests/filetests/lpfx/lp_random.glsl` (NEW)

Add tests for random functions:

- Test `lpfx_random` with various inputs
- Test deterministic behavior (same input = same output)
- Test different seeds produce different outputs

### File: `lp-glsl-filetests/filetests/lpfx/lp_srandom.glsl` (NEW)

Add tests for srandom functions:

- Test `lpfx_srandom` with various inputs
- Test output range is approximately [-1, 1]
- Test `lpfx_srandom3_tile` with tiling

### File: `lp-glsl-filetests/filetests/lpfx/lp_gnoise.glsl` (NEW)

Add tests for gnoise functions:

- Test `lpfx_gnoise` with various inputs
- Test output range is approximately [-1, 1] (or [0, 1] for tilable)
- Test `lpfx_gnoise3_tile` with tiling

### File: `lp-glsl-filetests/filetests/lpfx/lp_fbm.glsl` (NEW)

Add tests for fbm functions:

- Test `lpfx_fbm` with various octaves (1, 2, 4, 8)
- Test output range
- Test that more octaves produce more detail
- Test `lpfx_fbm3_tile` with tiling

## Success Criteria

- All test files created
- Tests verify basic functionality
- Tests verify deterministic behavior
- Tests verify output ranges
- All tests pass

## Code Organization

- Place helper utility functions **at the bottom** of files
- Place more abstract things, entry points, and tests **first**
- Keep related functionality grouped together

## Formatting

- Run `cargo +nightly fmt` on all changes before committing
- Ensure consistent formatting across modified files

## Language and Tone

- Keep language professional and restrained
- Avoid overly optimistic language like "comprehensive", "fully production ready", "complete solution"
- Avoid emoticons
- Use measured, factual descriptions of what was implemented
