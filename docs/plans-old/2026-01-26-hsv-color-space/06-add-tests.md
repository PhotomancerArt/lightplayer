# Phase 6: Add Comprehensive Tests

## Description

Add comprehensive tests for all HSV color space functions, covering basic conversions, round-trips, edge cases, and epsilon scenarios.

## Implementation

### Test Files

Add test modules to each implementation file:
- `lpfx/math/saturate_q32.rs` - Tests for saturate functions
- `lpfx/color/space/hue2rgb_q32.rs` - Tests for hue2rgb
- `lpfx/color/space/hsv2rgb_q32.rs` - Tests for hsv2rgb
- `lpfx/color/space/rgb2hsv_q32.rs` - Tests for rgb2hsv

### Test Cases

#### Saturate Tests
- Test clamping to [0, 1] range
- Test values below 0, above 1, and within range
- Test Vec3Q32 and Vec4Q32 variants

#### Hue2RGB Tests
- Test known hue values (0.0 = red, 0.333 = green, 0.666 = blue)
- Test hue wrapping (values > 1.0)
- Test edge cases (0.0, 1.0)

#### HSV2RGB Tests
- Test known HSV -> RGB conversions:
  - Pure red: HSV(0, 1, 1) -> RGB(1, 0, 0)
  - Pure green: HSV(0.333, 1, 1) -> RGB(0, 1, 0)
  - Pure blue: HSV(0.666, 1, 1) -> RGB(0, 0, 1)
  - Black: HSV(0, 0, 0) -> RGB(0, 0, 0)
  - White: HSV(0, 0, 1) -> RGB(1, 1, 1)
- Test Vec3Q32 and Vec4Q32 variants
- Test round-trip: RGB -> HSV -> RGB (should be approximately equal)

#### RGB2HSV Tests
- Test known RGB -> HSV conversions (inverse of HSV2RGB tests)
- Test edge cases:
  - Pure colors (red, green, blue)
  - Grayscale (equal RGB components)
  - Black (0, 0, 0)
  - White (1, 1, 1)
- **Epsilon case**: Test colors with very small or zero differences between RGB components
  - Colors where two components are nearly equal
  - Colors where all components are nearly equal (grayscale edge case)
  - Colors where one component dominates (very small differences)
- Test Vec3Q32 and Vec4Q32 variants
- Test round-trip: HSV -> RGB -> HSV (should be approximately equal)
- Validate HSV output range: H, S, V should be in [0, 1]

### Test Helpers

Use existing test helpers from `util/test_helpers.rs` for Q32 conversions.

## Success Criteria

- All functions have comprehensive test coverage
- Tests cover basic conversions, round-trips, edge cases, and epsilon scenarios
- All tests pass
- Epsilon case specifically tested for rgb2hsv
- Code formatted with `cargo +nightly fmt`

## Style Notes

### Code Organization

- Place helper utility functions **at the bottom** of files
- Place more abstract things, entry points, and tests **first**
- Keep related functionality grouped together

### Formatting

- Run `cargo +nightly fmt` on all changes before committing
- Ensure consistent formatting across modified files

### Language and Tone

- Keep language professional and restrained
- Avoid overly optimistic language like "comprehensive", "fully production ready", "complete solution"
- Avoid emoticons
- Code is never done, never perfect, never fully ready, never fully complete
- Use measured, factual descriptions of what was implemented
