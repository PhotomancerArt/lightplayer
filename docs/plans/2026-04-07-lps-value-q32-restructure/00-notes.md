# LpsValueQ32 Restructure Plan

## Scope of Work

Flesh out `LpsValueQ32` with proper conversions to/from `LpsValueF32`, and integrate it into the codebase along with `LpvmDataQ32`. Establish clean three-layer value representation:

- **LpsValueF32**: f32-centric semantic values (user/API level, JSON, tests)
- **LpsValueQ32**: Q32-centric semantic values (fixed-point exact representation)
- **LpvmDataQ32**: Byte-backed Q32 data in memory with layout rules (std430)

## Current State

### Files Overview

| File | Status | Description |
|------|--------|-------------|
| `lp-shader/lps-shared/src/lps_value_f32.rs` | Complete | Renamed from lps_value.rs. Full-featured f32 value type with equality, approx_eq, matrix tests |
| `lp-shader/lps-shared/src/lps_value_q32.rs` | Sketch | Basic enum structure using actual Q32 for floats, no derives or conversions yet |
| `lp-shader/lpvm/src/lpvm_data_q32.rs` | Complete | Byte-backed data with std430 layout, path access (get/set), works with LpsValueF32 |
| `lp-shader/lps-shared/src/lps_value_f64.rs` | To delete | Old f64 intermediate that we're replacing |
| `lp-shader/lps-shared/src/lps_value_f64_convert.rs` | To delete | Old conversion logic |

### LpsValueQ32 Current Sketch

```rust
pub enum LpsValueQ32 {
    I32(i32),
    U32(u32),
    F32(Q32),              // Actual Q32, not f64
    Bool(bool),
    Vec2([Q32; 2]),
    // ... arrays, structs
}
```

### LpvmDataQ32 Current State

Works with `LpsValueF32` currently. Reads/writes f32 values to byte buffers with std430 layout.

## Key Design Decisions Needed

### Q1: Should LpvmDataQ32 work with LpsValueQ32 or LpsValueF32?

**Context**: `LpvmDataQ32` stores Q32 values in memory (as bytes). Currently it converts to/from `LpsValueF32`.

**Option A**: Keep `LpvmDataQ32` working with `LpsValueF32` internally
- Pros: Single API for users, automatic conversion
- Cons: Less explicit, may hide precision issues

**Option B**: `LpvmDataQ32` works with `LpsValueQ32`
- Pros: Explicit about Q32 representation, matches the name
- Cons: Requires users to convert to/from LpsValueQ32

**Option C**: Support both with explicit methods
- `from_value_f32` / `to_value_f32` for LpsValueF32
- `from_value_q32` / `to_value_q32` for LpsValueQ32

**Answer**: Option C - explicit methods for both. This gives users convenience for f32 data while allowing exact Q32 control when needed.

### Q2: Conversion precision behavior

**Context**: Converting f32 → Q32 truncates (per Q32::from_f32). Some values lose precision.

**Question**: Should LpsValueF32→LpsValueQ32 conversion:

**Option A**: Truncate silently (current Q32::from_f32 behavior)
- Matches shader cast semantics
- No errors on conversion

**Option B**: Optionally report precision loss
- Add method `to_q32_with_info()` that reports if rounding occurred
- Useful for tests that want to know

**Option C**: Saturate out-of-range values
- Values outside Q32 range become MAX/MIN
- Matches shader behavior

**Answer**: Use saturating conversion. User updated Q32 to have `from_f32_saturating` (rounds + saturates) alongside `from_f32_wrapping` (truncates + wraps). LpsValueF32→LpsValueQ32 conversion uses saturating variant for safety. This aligns with Rust conventions (`saturating_add`, etc.) and prevents surprise wrapping.

### Q3: What traits should LpsValueQ32 implement?

**Context**: LpsValueF32 has `Clone`, `Debug`, custom `eq()`, `approx_eq()`.

**Question**: What does LpsValueQ32 need?

**Suggested minimum**:
- `#[derive(Clone, Debug, PartialEq)]` - basic traits
- `eq(&self, other) -> bool` - exact Q32 equality (raw bits match)
- `approx_eq(&self, other, tolerance_q32: Q32) -> bool` - approximate for floats
- `from_f32(&self) -> LpsValueF32` - conversion
- `to_q32(&self) -> LpsValueQ32` - conversion

Note: Q32 equality is exact by default since Q32 is fixed-point (i32 wrapper).

### Q4: ABI flattening location

**Context**: Currently `flatten_q32_arg` and `decode_q32_return` are in the old f64 module.

**Question**: Where should ABI marshaling live?

**Option A**: `lps-shared/src/lps_abi.rs` - dedicated ABI module
- Flatten: `LpsValueQ32` → `Vec<i32>` (raw words)
- Unflatten: `Vec<i32>` → `LpsValueQ32`

**Option B**: In `lps_value_q32.rs` as impl methods
- `LpsValueQ32::flatten(&self, ty: &LpsType) -> Vec<i32>`
- `LpsValueQ32::unflatten(ty: &LpsType, words: &[i32]) -> Self`

**Suggested**: Option A - separate ABI module keeps concerns clean. ABI is about calling convention, not the value type itself.

## Integration Points

### lpvm-cranelift
- `JitModule::call()` currently uses `LpsValueF64` - needs to use `LpsValueQ32`
- `lpvm_instance.rs` needs conversion f32 → Q32 for args, Q32 → f32 for returns

### lpvm-emu
- `EmuInstance::call()` same pattern as cranelift
- `glsl_q32_call_emulated()` uses old f64 types

### lps-filetests
- `q32_exec_common.rs` bridges old `GlslExecutable` trait to Q32 calls
- Needs update to use `LpsValueQ32` throughout

## Open Questions

1. Should we keep `GlslReturn<T>` generic or specialize to `GlslReturn<LpsValueQ32>` for ABI calls?
2. Should `LpsValueQ32` implement `Default`? (for zero-initializing)
3. How do we handle precision expectations in filetests when f32 → Q32 → f32 round-trip loses precision?

## Notes

- LpsValueF32 renamed from LpsValue for clarity
- lpvm_data_q32.rs renamed from lpvm_data.rs
- Both renames done by user already
- Need to ensure no broken imports
