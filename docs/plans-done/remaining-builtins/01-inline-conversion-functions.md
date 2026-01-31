# Phase 1: Inline Conversion Functions

## Goal
Implement inline conversions for simple functions (< 10 instructions) in the q32 transform.

## Functions to Implement
- **fract** (uses inline floor) - tests: `builtins/common-fract.glsl`
- **sign** (simple comparisons) - tests: `builtins/common-sign.glsl`
- **isinf/isnan** (always false for fixed-point) - tests: `builtins/common-isinf.glsl`, `builtins/common-isnan.glsl`

## Implementation Details

### fract
- Formula: `fract(x) = x - floor(x)`
- Uses existing inline floor conversion
- Add conversion function in `lightplayer/crates/lp-glsl-compiler/src/backend/transform/q32/converters/math.rs`

### sign
- Returns: 1.0 if x > 0, 0.0 if x == 0, -1.0 if x < 0
- Simple comparisons and selects
- Add conversion function in `lightplayer/crates/lp-glsl-compiler/src/backend/transform/q32/converters/math.rs`

### isinf/isnan
- Fixed-point doesn't have NaN/Inf, so always return false
- Simple constant return
- Add conversion functions in `lightplayer/crates/lp-glsl-compiler/src/backend/transform/q32/converters/math.rs`

## Files to Modify
- `lightplayer/crates/lp-glsl-compiler/src/backend/transform/q32/converters/math.rs` - add conversion functions
- `lightplayer/crates/lp-glsl-compiler/src/backend/transform/q32/instructions.rs` - wire up conversions

## Success Criteria
- All inline conversion functions implemented in transform
- Tests pass for fract, sign, isinf, isnan
- Code compiles without warnings
- Follow existing patterns (see convert_floor, convert_ceil in math.rs)

