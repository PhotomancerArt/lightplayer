# Phase 4: Add Semantic Checking

## Description

Create semantic checking module to recognize `lp_*` function names and map them to the appropriate `BuiltinId` variants. This enables type checking and signature validation for LP library functions.

## Implementation

### File: `frontend/semantic/lp_lib_fns.rs`

Create new module with:

1. **Function signature definitions**
   ```rust
   pub struct LpLibFnSignature {
       pub name: &'static str,
       pub param_types: Vec<Type>,
       pub return_type: Type,
   }
   ```

2. **Check function**
   ```rust
   pub fn is_lp_lib_fn(name: &str) -> bool
   ```
   Returns true if name starts with `lp_`

3. **Lookup function**
   ```rust
   pub fn lookup_lp_lib_fn(name: &str) -> Option<Vec<LpLibFnSignature>>
   ```
   Returns signatures for the function (handles overloads like `lpfx_hash`)

4. **Type check function**
   ```rust
   pub fn check_lp_lib_fn_call(name: &str, arg_types: &[Type]) -> Result<Type, String>
   ```
   Validates argument types and returns result type

### Function Signatures to Define

- `lpfx_hash(u32) -> u32`
- `lpfx_hash(u32, u32) -> u32`
- `lpfx_hash(u32, u32, u32) -> u32`
- `lpfx_snoise1(i32, u32) -> i32` (or `float, uint` -> `float` in GLSL types)
- `lpfx_snoise2(vec2, uint) -> float` (maps to `i32, i32, u32` internally)
- `lpfx_snoise3(vec3, uint) -> float` (maps to `i32, i32, i32, u32` internally)

### Integration

Update `frontend/semantic/mod.rs` to export the new module.

## Success Criteria

- Module compiles
- `is_lp_lib_fn()` correctly identifies `lp_*` functions
- `lookup_lp_lib_fn()` returns correct signatures
- `check_lp_lib_fn_call()` validates argument types correctly
- Handles vector types (vec2, vec3) correctly
- Code formatted with `cargo +nightly fmt`

## Notes

- Use GLSL types (float, vec2, vec3, uint) for user-facing signatures
- Map to internal types (i32, u32) during codegen
- Follow pattern from `frontend/semantic/builtins.rs`
