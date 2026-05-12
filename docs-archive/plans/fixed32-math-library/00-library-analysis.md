# Fixed-Point Math Library Analysis

## Libraries Analyzed

1. **libfixmath** (C) - Q16.16 format
2. **fr_math** (C) - Variable radix
3. **fpm** (C++) - Template-based, header-only

## libfixmath

### Format

- **Q16.16** fixed-point format (matches our q32 format!)
- Uses `int32_t` (typedef'd as `fix16_t`)
- `fix16_one = 0x00010000` (65536)

### Implementation Details

- **sin/cos**: Multiple implementations available:
  - **Taylor series** (default): Accurate to ~2.1%, uses 5 terms (x, x³/6, x⁵/120, x⁷/5040, x⁹/362880)
  - **Fast polynomial** (with `FIXMATH_FAST_SIN`): Faster, ~2.3% accuracy, uses 3-term polynomial
  - **Lookup table** (with `FIXMATH_SIN_LUT`): Uses 102KB lookup table
  - **Caching**: Optional 80KB cache for sin/atan (can be disabled with `FIXMATH_NO_CACHE`)
- **cos**: Implemented as `sin(x + π/2)`
- Uses modular arithmetic to reduce input to [-π, π] range
- No dependencies (pure C, no stdlib needed for core functions)

### Pros

- ✅ **Perfect format match**: Q16.16 exactly matches our q32 format
- ✅ **No dependencies**: Pure C, no-std compatible
- ✅ **Multiple accuracy/speed options**: Can choose based on needs
- ✅ **Well-tested**: Has benchmarks and tests
- ✅ **Simple API**: Direct function calls, easy to port
- ✅ **MIT license**: Permissive

### Cons

- ⚠️ **Large lookup table**: 102KB if using LUT option (probably not needed)
- ⚠️ **Caching uses static memory**: 80KB if enabled (can be disabled)
- ⚠️ **Not actively maintained**: But code is stable

### Code Complexity

- **Low**: Straightforward C code, easy to port to Rust
- Uses standard fixed-point operations (multiply, divide, shift)
- No complex algorithms beyond Taylor series

### Recommendation

**⭐⭐⭐⭐⭐ Best fit** - Perfect format match, simple implementation, easy to port.

---

## fr_math

### Format

- **Variable radix**: Programmer chooses radix point (e.g., s11.4, s10.5)
- Uses `s32` (signed 32-bit) as base type
- More flexible but requires tracking radix per operation

### Implementation Details

- **CORDIC algorithm**: Uses 32-bit CORDIC for trig functions
- **cordic_trig_sin_cos32.c**: Provides sin/cos via CORDIC
- CORDIC uses only shifts and adds (no multiplies needed)
- Requires lookup table of 32 angles (small, ~128 bytes)
- Valid for range [-π/2, π/2], needs range reduction

### Pros

- ✅ **CORDIC algorithm**: Very efficient (only shifts/adds)
- ✅ **No multiplies**: Good for platforms without hardware multiplier
- ✅ **Small lookup table**: Only 32 entries (~128 bytes)
- ✅ **No dependencies**: Pure C
- ✅ **BSD-2 license**: Permissive

### Cons

- ⚠️ **Variable radix**: More complex API, need to track radix
- ⚠️ **Not Q16.16**: Would need adaptation for our format
- ⚠️ **CORDIC complexity**: More complex algorithm to understand/port
- ⚠️ **Range reduction needed**: Only works for [-π/2, π/2]

### Code Complexity

- **Medium**: CORDIC is more complex than Taylor series
- Requires understanding of iterative algorithm
- Range reduction logic needed

### Recommendation

**⭐⭐⭐ Good alternative** - CORDIC is efficient but more complex. Could be useful if we need maximum performance without multiplies.

---

## fpm

### Format

- **Template-based**: `fixed<B, I, F, R>` where F is fractional bits
- C++ header-only library
- Can use `fixed_16_16` which is Q16.16

### Implementation Details

- **sin**: Fifth-order curve-fitting approximation (Jasper Vijn's method)
  - Worst-case relative error: 0.07% over [-π, π]
  - Reduces domain to [0, 1] then applies polynomial
- **cos**: Implemented as `sin(x ± π/2)` with overflow protection
- Uses template metaprogramming for compile-time constants

### Pros

- ✅ **High accuracy**: 0.07% error (better than libfixmath's 2.1%)
- ✅ **Modern C++**: Type-safe, template-based
- ✅ **Header-only**: Easy to include
- ✅ **Well-documented**: Has accuracy/performance docs

### Cons

- ⚠️ **C++ only**: Would need to port to Rust (no direct C compatibility)
- ⚠️ **Template complexity**: More complex to understand/port
- ⚠️ **Requires C++11+**: Not pure C
- ⚠️ **More complex code**: Template metaprogramming harder to port

### Code Complexity

- **High**: Template metaprogramming, C++ specific features
- Would need significant adaptation for Rust
- More abstract than direct C implementation

### Recommendation

**⭐⭐ Good reference** - High accuracy but C++ complexity makes it harder to port. Could use as reference for the polynomial coefficients.

---

## Comparison Summary

| Feature             | libfixmath   | fr_math       | fpm       |
| ------------------- | ------------ | ------------- | --------- |
| **Format Match**    | ✅ Q16.16    | ⚠️ Variable   | ✅ Q16.16 |
| **Language**        | C            | C             | C++       |
| **Dependencies**    | None         | None          | C++11+    |
| **Accuracy**        | ~2.1%        | Good (CORDIC) | 0.07%     |
| **Code Complexity** | Low          | Medium        | High      |
| **Portability**     | Easy         | Medium        | Hard      |
| **License**         | MIT          | BSD-2         | MIT       |
| **Maintenance**     | Low activity | Active        | Active    |

## Recommendation

**Use libfixmath as primary reference** for the following reasons:

1. **Perfect format match**: Q16.16 exactly matches our q32 format
2. **Simple to port**: Pure C, straightforward code
3. **No dependencies**: Works in no-std Rust
4. **Multiple options**: Can choose accuracy vs speed
5. **Proven**: Well-tested, stable code

**Implementation approach**:

- Port libfixmath's Taylor series implementation (default, accurate version)
- Use `FIXMATH_NO_CACHE` mode (no static memory)
- Adapt to work with raw `i32` values (no type wrapper needed)
- Can reference fpm's polynomial for higher accuracy if needed later

**Optional enhancement**:

- Could also port fr_math's CORDIC as an alternative implementation if we need maximum performance without multiplies, but libfixmath should be sufficient for initial implementation.
