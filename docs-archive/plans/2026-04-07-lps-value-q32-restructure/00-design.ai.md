# LpsValueQ32 Design

## Scope

Flesh out `LpsValueQ32` with proper conversions to/from `LpsValueF32`, and integrate it into the codebase along with `LpvmDataQ32`. Establish clean three-layer value representation with explicit Q32 semantics.

## File Structure

```
lp-shader/
├── lps-shared/src/
│   ├── lib.rs                          # UPDATE: Export LpsValueQ32, LpsValueF32
│   ├── lps_value_f32.rs                # EXISTING (renamed): F32 semantic values
│   ├── lps_value_q32.rs                # UPDATE: Q32 type + conversions to/from F32
│   ├── lps_value_f64.rs                # DELETE
│   └── lps_value_f64_convert.rs        # DELETE
│
├── lpvm/src/
│   ├── lib.rs                          # UPDATE: Export abi module
│   ├── lpvm_data_q32.rs                # UPDATE: Add from_q32/to_q32 methods
│   └── lpvm_abi.rs                     # NEW: ABI marshaling (flatten/unflatten)
│
├── lpvm-cranelift/src/
│   ├── lib.rs                          # UPDATE: Export LpsValueQ32 instead of F64
│   ├── call.rs                         # UPDATE: Use LpsValueQ32 in JitModule::call
│   └── lpvm_instance.rs                # UPDATE: Use new conversion functions
│
├── lpvm-emu/src/
│   ├── emu_run.rs                      # UPDATE: Use LpsValueQ32 in glsl_q32_call_emulated
│   └── instance.rs                     # UPDATE: Use new conversion functions
│
└── lps-filetests/src/test_run/
    ├── q32_exec_common.rs              # UPDATE: Use LpsValueQ32 throughout
    ├── lpir_jit_executable.rs          # UPDATE: Use LpsValueQ32 in call_q32_ret
    └── lpir_rv32_executable.rs         # UPDATE: Use LpsValueQ32 in call_q32_ret
```

## Conceptual Architecture

Three-layer value representation with clean separation:

```
┌─────────────────────────────────────────────────────────────────┐
│ LpsValueF32 (User/API Layer)                                    │
│ - F32 components (f32, [f32;N], etc.)                           │
│ - JSON, test inputs, user data                                  │
│ - approx_eq() for tolerance-based comparison                    │
└──────────────────┬──────────────────────────────────────────────┘
                   │ lps_value_q32 (module)
                   │ Uses Q32::from_f32_saturating()
                   ▼
┌─────────────────────────────────────────────────────────────────┐
│ LpsValueQ32 (Semantic Q32 Layer)                                │
│ - Q32 components (Q32 type, not f32)                            │
│ - Exact representation, preserves precision intent              │
│ - eq() is exact bit comparison                                  │
│ - Conversion functions: lps_value_to_q32(), q32_to_lps_value()  │
└──────────────────┬──────────────────────────────────────────────┘
                   │ lpvm::abi (module)
                   │ Uses Q32::to_fixed() / Q32::from_fixed()
                   ▼
┌─────────────────────────────────────────────────────────────────┐
│ LpvmDataQ32 / Vec<i32> (ABI/Memory Layer)                       │
│ - Raw bytes (std430 layout) or flat i32 words                   │
│ - JIT/emulator calling convention                               │
│ - No semantics, just bits                                       │
└─────────────────────────────────────────────────────────────────┘
```

## Conversion Semantics

### F32 → Q32 (LpsValueF32 → LpsValueQ32)

Uses `Q32::from_f32_saturating()`:
- Rounds to nearest representable Q32 value
- Saturates at Q32 limits (`±32767.9999...`)
- Safe for user input - no surprise wrapping

```rust
LpsValueF32::F32(1.5) → LpsValueQ32::F32(Q32(98304))  // 1.5 * 65536
LpsValueF32::F32(50000.0) → LpsValueQ32::F32(Q32(0x7FFF_FFFF))  // saturated
```

### Q32 → F32 (LpsValueQ32 → LpsValueF32)

Uses `Q32::to_f32()`:
- Exact conversion (no precision loss in representable range)
- Inverse of saturating conversion (may not round-trip exact f32 values)

### Q32 ↔ i32 Raw (ABI encoding)

Uses `Q32::to_fixed()` / `Q32::from_fixed()`:
- `to_fixed()` returns raw i32: `Q32(98304).to_fixed() == 98304_i32`
- `from_fixed()` wraps raw i32: `Q32::from_fixed(98304) == Q32(98304)`
- Integers pass through directly (no Q32 wrapping)

## Main Components

### LpsValueQ32 (in lps_value_q32.rs)

```rust
#[derive(Clone, Debug, PartialEq)]
pub enum LpsValueQ32 {
    I32(i32),
    U32(u32),
    F32(Q32),              // Actual Q32, not f64
    Bool(bool),
    Vec2([Q32; 2]),
    Vec3([Q32; 3]),
    Vec4([Q32; 4]),
    IVec2([i32; 2]),
    IVec3([i32; 3]),
    IVec4([i32; 4]),
    UVec2([u32; 2]),
    UVec3([u32; 3]),
    UVec4([u32; 4]),
    BVec2([bool; 2]),
    BVec3([bool; 3]),
    BVec4([bool; 4]),
    Mat2x2([[Q32; 2]; 2]), // Column-major per GLSL
    Mat3x3([[Q32; 3]; 3]),
    Mat4x4([[Q32; 4]; 4]),
    Array(Box<[LpsValueQ32]>),
    Struct {
        name: Option<String>,
        fields: Vec<(String, LpsValueQ32)>,
    },
}

// Conversions to/from LpsValueF32 (at bottom of file)
pub fn lps_value_to_q32(ty: &LpsType, v: &LpsValueF32) -> Result<LpsValueQ32, CallError>
pub fn q32_to_lps_value(ty: &LpsType, v: LpsValueQ32) -> Result<LpsValueF32, CallError>
```

### LpvmAbi (in lpvm/src/abi.rs)

```rust
/// Flatten LpsValueQ32 to raw i32 words for JIT/emulator ABI
pub fn flatten_q32(ty: &LpsType, v: &LpsValueQ32) -> Result<Vec<i32>, CallError>

/// Unflatten raw i32 words from JIT/emulator to LpsValueQ32
pub fn unflatten_q32(ty: &LpsType, words: &[i32]) -> Result<LpsValueQ32, CallError>

/// Error type for conversion failures
pub enum CallError { ... }

/// Generic return wrapper (reused from old implementation)
pub struct GlslReturn<V> {
    pub value: Option<V>,
    pub outs: Vec<V>,
}
```

### LpvmDataQ32 (updated in lpvm_data_q32.rs)

Add methods for Q32 value support:

```rust
impl LpvmDataQ32 {
    // Existing F32 methods
    pub fn from_value_f32(ty: &LpsType, v: &LpsValueF32) -> Result<Self, DataError>
    pub fn to_value_f32(&self) -> Result<LpsValueF32, DataError>
    
    // New Q32 methods
    pub fn from_value_q32(ty: &LpsType, v: &LpsValueQ32) -> Result<Self, DataError>
    pub fn to_value_q32(&self) -> Result<LpsValueQ32, DataError>
}
```

### JIT/Emulator Integration

`JitModule::call()` and `EmuInstance::call()` use `LpsValueQ32` for their interface:

```rust
// In lpvm-cranelift/src/call.rs
impl JitModule {
    pub fn call(&self, name: &str, args: &[LpsValueQ32]) 
        -> Result<GlslReturn<LpsValueQ32>, CallError> 
    {
        // Flatten args to i32 words
        let flat_args: Vec<i32> = args.iter()
            .zip(params.iter())
            .map(|(arg, param)| flatten_q32(&param.ty, arg))
            .collect()?;
        
        // ... invoke JIT function ...
        
        // Unflatten return
        let value = unflatten_q32(&return_type, &return_words)?;
        Ok(GlslReturn { value: Some(value), outs: vec![] })
    }
}
```

The `LpvmInstance::call()` trait implementation handles LpsValueF32 → LpsValueQ32 conversion before delegating to the module-level call.

## Interaction Summary

1. **User/API**: Works with `LpsValueF32` (f32 values, JSON, etc.)
2. **Instance::call()**: Converts `LpsValueF32` → `LpsValueQ32` via `lps_value_to_q32()`
3. **JitModule::call()**: Flattens `LpsValueQ32` → `Vec<i32>` via `flatten_q32()`
4. **JIT/Emulator**: Executes with raw i32 words
5. **Return**: Unflatten `Vec<i32>` → `LpsValueQ32` via `unflatten_q32()`
6. **Instance::call()**: Converts `LpsValueQ32` → `LpsValueF32` via `q32_to_lps_value()`
7. **User/API**: Receives `LpsValueF32`

This keeps the Q32 semantics explicit in the middle layer while presenting f32 to users.

## Follow-up: `LpvmInstance::call_q32` (M5 / filetests)

Filetests and other hosts may need to call Q32 shaders **without** representing float parameters as `f32` first. The **M5** plan adds **`LpvmInstance::call_q32`** with **flat `i32` ABI words** (same layout as `lpvm_abi` flattening), implemented by reusing this stack — not a second calling convention. See [`../../plans-done/2026-04-07-lpvm2-m5-filetests/00-notes.md`](../plans-done/2026-04-07-lpvm2-m5-filetests/00-notes.md).
