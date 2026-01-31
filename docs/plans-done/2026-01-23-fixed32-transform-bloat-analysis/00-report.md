# Q32 Transform Code Bloat Analysis Report

## Executive Summary

The q32 transform is causing massive code bloat, increasing instruction counts by 3-4x. Analysis of
the `hash` and `hsv_to_rgb` functions shows that simple operations like addition and subtraction
generate 20+ instructions each due to inline saturation checks. This bloat is causing memory
allocation failures on ESP32 targets.

**Key Findings:**

- `hash` function: 15 → 51 instructions (+240%)
- `hsv_to_rgb` function: 79 → 340 instructions (+330%)
- Primary cause: Inline saturation checks on every arithmetic operation
- Solution: Move saturation logic to builtin functions

## Problem Statement

The q32 transform converts floating-point operations to fixed-point arithmetic, but generates
excessive code due to inline saturation checks. This causes:

1. **Memory pressure**: 3-4x increase in CLIF IR size leads to allocation failures
2. **Compilation slowdown**: More instructions to process during Cranelift compilation
3. **Runtime overhead**: More instructions to execute (though this is secondary to memory issues)

## Root Cause Analysis

### 1. Inline Saturation Checks on Arithmetic Operations

**Current Implementation:**

- `fadd` and `fsub` generate ~20 instructions each
- Every operation includes:
    - 6 comparisons (sign checks for both operands and result)
    - 4 `band` operations (combining conditions)
    - 2 `select` operations (choosing saturation values)
    - 2 clamp operations (`smin`/`smax`)

**Example from `hash` function:**

```clif
// Before: Simple addition
v6 = fadd v4, v5

// After: 20+ instructions with saturation
v6 = iadd v4, v5
v7 = iconst.i32 0
v8 = iconst.i32 0x7fff_0000
v9 = iconst.i32 -2147483648
v10 = icmp sge v4, v7
v11 = icmp sge v5, v7
v12 = icmp slt v6, v7
v13 = band v10, v11
v14 = band v13, v12
// ... 10+ more instructions for saturation
```

**Impact:**

- `hash` function has 2 `fadd` operations → ~40 extra instructions
- `hsv_to_rgb` has many arithmetic operations → hundreds of extra instructions

### 2. Complex Division Handling

**Current Implementation:**

- `fdiv` generates ~30+ instructions per operation
- Includes:
    - Zero checking (multiple paths)
    - Sign checking
    - Two division paths (shifted divisor vs full divisor) - handles edge case for small divisors <
      2^16
    - Saturation logic

**Issue:**

- `__lp_q32_div` builtin exists but is intentionally NOT used
- Transform generates inline code to handle edge cases that the builtin may not handle correctly
- Code comment mentions "bug fix for small divisors < 2^16"
- Test for `fdiv` is currently ignored due to "known issue with the division algorithm"
- The inline code handles division-by-zero saturation and small divisor edge cases

**Impact:**

- Every division operation adds 30+ instructions
- `hsv_to_rgb` has multiple divisions → massive bloat
- **Note**: This may be intentional to handle edge cases, but still contributes significantly to
  bloat

### 3. Boolean-to-Fixed-Point Conversion Overhead

**Current Implementation:**

- Every `fcmp` adds a multiply-by-65536 to convert boolean (i8) to fixed-point (i32)
- Adds 2-3 instructions per comparison

**Example:**

```clif
// Comparison result is i8 (0 or 1)
v19 = icmp lt v1, v18

// Convert to fixed-point (multiply by 65536)
v20 = sextend.i32 v19
v21 = iconst.i32 65536
v22 = imul v20, v21
```

**Impact:**

- Every comparison adds 2-3 instructions
- Conditional code paths multiply this overhead

### 4. Missing Builtin Functions for Basic Arithmetic

**Current State:**

- Only `fmul` uses a builtin (`__lp_q32_mul`)
- `fadd`, `fsub`, `fdiv` generate inline code
- Builtins exist for math functions (sin, cos, etc.) but not for basic arithmetic

**Available Builtins:**

- `__lp_q32_mul` ✅ (used)
- `__lp_q32_div` ✅ (exists but intentionally not used - inline code handles edge cases)
- `__lp_q32_add` ❌ (does not exist)
- `__lp_q32_sub` ❌ (does not exist)

### 5. Vector Operations Are Unoptimized

**Current State:**

- Vectors are just sequences of scalar operations
- No special handling or builtin functions for vector operations
- A `vec2` operation = 2 scalar operations, each with full saturation checks

**Example:**

```glsl
vec2 a = vec2(1.0, 2.0);
vec2 b = vec2(3.0, 4.0);
vec2 c = a + b;  // Generates 2 separate fadd operations, each with 20+ instructions
```

**Impact:**

- Vector operations multiply the bloat
- `perlin_noise` function uses many vec2 operations → significant overhead

## Detailed Bloat Breakdown

### Function: `hash`

- **Before**: 15 instructions, 656 bytes CLIF text
- **After**: 51 instructions, 2123 bytes CLIF text
- **Increase**: +240% instructions, +223% size

**Operations causing bloat:**

- 2 `fadd` operations → ~40 extra instructions
- 1 `fmod` call → conversion overhead
- 1 `sin` call → conversion overhead
- 1 `fmul` operation → uses builtin (good)

### Function: `hsv_to_rgb`

- **Before**: 79 instructions, 6189 bytes CLIF text
- **After**: 340 instructions, 25385 bytes CLIF text
- **Increase**: +330% instructions, +310% size

**Operations causing bloat:**

- Multiple `fmul` operations → each generates saturation checks
- Multiple `fsub` operations → each generates 20+ instructions
- Multiple `fadd` operations → each generates 20+ instructions
- Complex conditional logic → each comparison adds overhead
- Division operations → complex inline code

### Function: `main`

- Contains many vector operations
- Each vector operation expands to multiple scalar operations
- All scalar operations include saturation checks

## Current Builtin Infrastructure

### Available Builtins

- **Arithmetic**: `mul`, `div`
- **Math functions**: `sin`, `cos`, `tan`, `asin`, `acos`, `atan`, `sinh`, `cosh`, `tanh`, `asinh`,
  `acosh`, `atanh`
- **Exp/Log**: `exp`, `exp2`, `log`, `log2`, `pow`
- **Other**: `sqrt`, `fma`, `round`, `roundeven`, `mod`, `inversesqrt`, `ldexp`

### Builtin Pattern

Builtins handle saturation internally, reducing code size:

- `__lp_q32_mul`: Single call replaces inline multiplication + saturation
- Builtins are optimized implementations that handle edge cases efficiently

## Proposed Solutions

### Solution 1: Create Builtin Functions for `fadd` and `fsub` (High Priority)

**Approach:**

- Implement `__lp_q32_add` and `__lp_q32_sub` builtins
- Move saturation logic into builtins
- Update transform to call builtins instead of generating inline code

**Expected Impact:**

- Reduces ~20 instructions to 1 call per operation
- Estimated 70-80% reduction in arithmetic-heavy code
- Immediate fix for the most common operations

**Implementation:**

- Add builtin implementations in `lp-glsl-builtins/src/builtins/q32/`
- Register in builtin registry
- Update `convert_fadd` and `convert_fsub` to use builtins

### Solution 2: Use Builtin for `fdiv` OR Improve Builtin (Medium-High Priority)

**Current Situation:**

- `__lp_q32_div` exists but is intentionally not used
- Inline code handles edge cases: division-by-zero, small divisors (< 2^16)
- Test is ignored due to "known issue with the division algorithm"

**Approach A: Fix Builtin and Use It**

- Investigate why builtin has issues with edge cases
- Fix `__lp_q32_div` to handle division-by-zero and small divisors correctly
- Update `convert_fdiv` to use builtin (like `convert_fmul`)

**Approach B: Optimize Inline Code**

- Keep inline code but optimize it
- Reduce redundant checks
- Simplify the two-path division logic if possible

**Expected Impact:**

- Approach A: Reduces ~30 instructions to 1 call per operation (90%+ reduction)
- Approach B: Reduces ~30 instructions to ~15-20 instructions (30-50% reduction)

**Implementation:**

- **For Approach A:**
    - Analyze builtin implementation and edge case handling
    - Fix builtin to match inline code's edge case handling
    - Update `convert_fdiv` to follow same pattern as `convert_fmul`
- **For Approach B:**
    - Refactor inline code to reduce redundancy
    - Combine common paths where possible

### Solution 3: Optimize Comparison Conversion (Medium Priority)

**Approach:**

- Consider keeping comparisons as i8 booleans where possible
- Only convert to fixed-point when needed for arithmetic
- Use select operations directly with boolean results

**Expected Impact:**

- Saves 2-3 instructions per comparison
- Reduces overhead in conditional code paths
- May require changes to type system

**Implementation:**

- Analyze where boolean-to-fixed conversion is actually needed
- Update comparison conversion to be context-aware
- May need to track boolean values separately

### Solution 4: Vector/Matrix Builtin Functions (Long-term)

**Approach:**

- Create builtins for `vec2_add`, `vec3_add`, `vec4_add`, etc.
- Handle saturation once per vector instead of per component
- Optimize common vector operations

**Expected Impact:**

- 50-70% reduction for vector-heavy code
- Significant improvement for shaders using many vector operations
- Better performance due to optimized implementations

**Implementation:**

- Design vector builtin API
- Implement vector operations with saturation
- Update GLSL codegen to use vector builtins where applicable

### Solution 5: Conditional Saturation (Optional)

**Approach:**

- Add a flag to disable saturation checks in performance-critical paths
- Use wrapping arithmetic where overflow is acceptable
- Trade-off: smaller code vs. overflow safety

**Expected Impact:**

- 50-70% reduction when saturation is disabled
- Useful for performance-critical shaders
- Requires careful analysis of overflow behavior

**Implementation:**

- Add `saturate` flag to `Q32Transform`
- Update arithmetic converters to check flag
- Use wrapping arithmetic when flag is false

## Impact Estimates

### Current Bloat

- **hash**: 15 → 51 instructions (+240%)
- **hsv_to_rgb**: 79 → 340 instructions (+330%)
- **Overall**: 3-4x increase in code size

### With Proposed Optimizations

**Solution 1 + 2 (Builtins for add/sub/div):**

- Estimated 50-70% reduction in arithmetic-heavy code
- Would reduce `hash` from 51 to ~25-30 instructions
- Would reduce `hsv_to_rgb` from 340 to ~150-200 instructions

**Solution 3 (Comparison optimization):**

- Additional 10-15% reduction
- Most impactful in conditional-heavy code

**Solution 4 (Vector builtins):**

- Additional 30-50% reduction for vector-heavy code
- Most impactful for shaders using many vector operations

**Combined Impact:**

- Could reduce bloat from 3-4x to 1.5-2x
- Would significantly reduce memory pressure on ESP32
- Would improve compilation speed

## Recommendations

### Immediate Actions (High Priority)

1. **Implement `__lp_q32_add` and `__lp_q32_sub` builtins**
    - Highest impact, addresses most common operations
    - Follows existing pattern (like `mul`)
    - Estimated effort: 1-2 days

2. **Investigate and fix `__lp_q32_div` builtin OR optimize inline division code**
    - Builtin exists but has known issues with edge cases
    - Option A: Fix builtin to handle edge cases, then use it (highest impact)
    - Option B: Optimize inline code to reduce bloat (lower impact but safer)
    - Estimated effort: 1-2 days (Option A) or 0.5-1 day (Option B)

### Short-term Actions (Medium Priority)

3. **Optimize comparison conversion**
    - Analyze where boolean-to-fixed conversion is needed
    - Implement context-aware conversion
    - Estimated effort: 2-3 days

### Long-term Actions (Lower Priority)

4. **Implement vector builtin functions**
    - Design API and implement common operations
    - Update codegen to use vector builtins
    - Estimated effort: 1-2 weeks

5. **Consider conditional saturation**
    - Evaluate use cases for wrapping arithmetic
    - Implement flag-based saturation control
    - Estimated effort: 3-5 days

## Conclusion

The q32 transform's code bloat is primarily caused by inline saturation checks on every arithmetic
operation. Moving this logic to builtin functions (following the pattern already established for
`mul`) will significantly reduce code size and memory pressure. The highest-impact solutions are
implementing builtins for `add` and `sub`, and using the existing `div` builtin.

These changes should reduce the 3-4x bloat to approximately 1.5-2x, making the transform viable for
memory-constrained targets like ESP32.
