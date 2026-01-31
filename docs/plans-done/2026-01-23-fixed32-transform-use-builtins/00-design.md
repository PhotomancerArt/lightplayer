# Design: Make Q32 Transform Use Builtins for Add, Sub, and Div

## Overview

Update the q32 transform to use builtin functions for `add`, `sub`, and `div` operations instead of
generating inline saturation code. This will reduce code bloat from ~20-30 instructions per
operation to a single function call, following the same pattern already established for `mul`.

## File Structure

```
lp-glsl/lp-glsl-builtins/src/builtins/q32/
├── add.rs                    # NEW: __lp_q32_add builtin implementation
├── sub.rs                    # NEW: __lp_q32_sub builtin implementation
├── div.rs                    # EXISTS: Verify edge case handling
├── mul.rs                    # EXISTS: Reference implementation pattern
└── mod.rs                    # AUTO-GENERATED: Will include add/sub exports

lp-glsl/lp-glsl-compiler/src/backend/transform/q32/converters/
└── arithmetic.rs             # UPDATE: convert_fadd, convert_fsub, convert_fdiv to use builtins

lp-glsl/lp-glsl-compiler/src/backend/builtins/
└── registry.rs               # AUTO-GENERATED: Will include Q32Add, Q32Sub

lp-glsl/lp-glsl-builtins-emu-app/src/
└── builtin_refs.rs           # AUTO-GENERATED: Will include add/sub references
```

## Code Structure

### New Builtin Functions

**__lp_q32_add(a: i32, b: i32) -> i32**

- Fixed-point addition with saturation
- Use i64 for intermediate calculation to avoid overflow
- Clamp result to [MIN_FIXED, MAX_FIXED]
- Return saturated i32
- Pattern: Similar to `__lp_q32_mul` but simpler (no shift needed)

**__lp_q32_sub(a: i32, b: i32) -> i32**

- Fixed-point subtraction with saturation
- Use i64 for intermediate calculation to avoid overflow
- Clamp result to [MIN_FIXED, MAX_FIXED]
- Return saturated i32
- Pattern: Similar to `__lp_q32_add` but subtract instead of add

**__lp_q32_div(dividend: i32, divisor: i32) -> i32**

- EXISTS: Already implemented
- Verify it handles edge cases correctly (division-by-zero, small divisors)
- If issues found, fix before using

### Updated Transform Functions

**convert_fadd()**

- UPDATE: Replace inline saturation code (~20 instructions) with builtin call
- Pattern: Follow `convert_fmul` implementation
- Get FuncId from func_id_map
- Create signature: (i32, i32) -> i32
- Call builtin function

**convert_fsub()**

- UPDATE: Replace inline saturation code (~20 instructions) with builtin call
- Pattern: Follow `convert_fmul` implementation
- Get FuncId from func_id_map
- Create signature: (i32, i32) -> i32
- Call builtin function

**convert_fdiv()**

- UPDATE: Replace inline division code (~30 instructions) with builtin call
- Pattern: Follow `convert_fmul` implementation
- Get FuncId from func_id_map
- Create signature: (i32, i32) -> i32
- Call builtin function
- Remove special handling for small divisors (builtin should handle it)

## Implementation Details

### Builtin Implementation Pattern

Both `add` and `sub` will follow this pattern (similar to `mul`):

```rust
#[unsafe(no_mangle)]
pub extern "C" fn __lp_q32_add(a: i32, b: i32) -> i32 {
    // Use i64 for intermediate calculation
    let a_wide = a as i64;
    let b_wide = b as i64;
    
    // Perform operation
    let result_wide = a_wide + b_wide;  // or - for sub
    
    // Saturate to fixed-point range
    if result_wide > MAX_FIXED as i64 {
        MAX_FIXED
    } else if result_wide < MIN_FIXED as i64 {
        MIN_FIXED
    } else {
        result_wide as i32
    }
}
```

### Transform Update Pattern

All three converters will follow the `convert_fmul` pattern:

1. Extract operands from old instruction
2. Map operands using value_map
3. Get FuncId from func_id_map using builtin name
4. Create signature: (i32, i32) -> i32
5. Create UserExternalName with FuncId
6. Import external function (colocated: false)
7. Call builtin with mapped arguments
8. Map result back to old result value

### Builtin Generation

After creating `add.rs` and `sub.rs`:

1. Run `scripts/build-builtins.sh` to auto-generate:
    - `mod.rs` exports
    - `registry.rs` enum variants and mappings
    - `builtin_refs.rs` function references

### Testing

1. Unignore `test_q32_fdiv` test in `arithmetic.rs`
2. Run filetests to verify correctness
3. Run lp-glsl-q32-metrics-app script to compare code sizes

## Constants

- `MAX_FIXED: i32 = 0x7FFF_FFFF` (maximum representable fixed-point value)
- `MIN_FIXED: i32 = i32::MIN` (minimum representable fixed-point value)

## Success Criteria

- `__lp_q32_add` and `__lp_q32_sub` builtins implemented
- `convert_fadd`, `convert_fsub`, `convert_fdiv` use builtins instead of inline code
- Builtin registry auto-generated with new entries
- All tests pass (including unignored `test_q32_fdiv`)
- Code size reduction verified via lp-glsl-q32-metrics-app comparison
- Code formatted with `cargo +nightly fmt`
