# Phase 4: Test and Verify

## Description

Run tests to verify that vector return support works correctly. Ensure existing functionality is not broken.

## Changes

- Run GLSL filetests for HSV functions
- Run existing LPFX tests to ensure no regressions
- Test both Q32 and F32 variants
- Test Vec2, Vec3, and Vec4 returns

## Success Criteria

- GLSL filetests for `lp_hue2rgb`, `lp_hsv2rgb`, `lp_rgb2hsv`, `lp_saturate` pass
- Existing LPFX tests (hash, simplex) continue to pass
- No panics when calling vector-returning LPFX functions
- Code compiles without warnings

## Implementation Notes

- Run: `scripts/glsl-filetests.sh lpfx`
- Verify all test files pass
- Check for any compiler warnings or errors
