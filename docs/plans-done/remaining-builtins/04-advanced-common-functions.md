# Phase 4: Advanced Common Functions

## Goal

Implement complex common functions: fma, frexp, ldexp, modf.

## Functions to Implement

- **fma** (fused multiply-add) - tests: `builtins/common-fma.glsl` ✅
- **ldexp** (scale by power of 2) - tests: `builtins/common-ldexp.glsl` ✅
- **frexp** (extract mantissa and exponent) - tests: `builtins/common-frexp.glsl` ⏸️ **DEFERRED** -
  requires output parameter support (Phase 8)
- **modf** (split into integer and fractional parts) - tests: `builtins/common-modf.glsl` ⏸️ *
  *DEFERRED** - requires output parameter support (Phase 8)

## Implementation Details

### fma

- Formula: `fma(a, b, c) = a * b + c` (with higher precision)
- Uses existing mul and add operations
- For fixed-point: can use mul + add, but may need special handling for precision
- Reference: fpm library or standard algorithm
- File: `lightplayer/crates/lp-glsl-builtins/src/q32/fma.rs` (new file)

### frexp

- Formula: `frexp(x) = (mantissa, exp)` where `x = mantissa * 2^exp`, `0.5 <= |mantissa| < 1.0`
- Returns mantissa, sets exp via output parameter
- For fixed-point: extract exponent from bit pattern, normalize mantissa
- Reference: libfixmath or fpm library
- File: `lightplayer/crates/lp-glsl-builtins/src/q32/frexp.rs` (new file)
- Note: GLSL frexp uses output parameter - may need special handling

### ldexp

- Formula: `ldexp(x, exp) = x * 2^exp`
- For fixed-point: shift left/right by exp bits
- Simple implementation: shift operations
- File: `lightplayer/crates/lp-glsl-builtins/src/q32/ldexp.rs` (new file)

### modf

- Formula: `modf(x) = (fractional, integer)` where both have same sign as x
- Split into integer and fractional parts
- Uses floor for integer part, subtract for fractional
- File: `lightplayer/crates/lp-glsl-builtins/src/q32/modf.rs` (new file)
- Note: GLSL modf uses output parameter - may need special handling

## Files to Create/Modify

- `lightplayer/crates/lp-glsl-builtins/src/q32/fma.rs` - new file
- `lightplayer/crates/lp-glsl-builtins/src/q32/frexp.rs` - new file
- `lightplayer/crates/lp-glsl-builtins/src/q32/ldexp.rs` - new file
- `lightplayer/crates/lp-glsl-builtins/src/q32/modf.rs` - new file
- Run builtin generator to update boilerplate

## Success Criteria

- All functions implemented following existing patterns
- Unit tests pass with appropriate tolerance (3% for complex functions)
- Functions registered in builtin generator (auto-generated)
- Code compiles without warnings
- Tests pass for all four functions

