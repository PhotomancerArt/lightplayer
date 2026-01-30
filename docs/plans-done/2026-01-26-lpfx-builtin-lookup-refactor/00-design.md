# Design: LPFX Builtin Lookup Refactor

## Overview

Refactor the Q32 transform to use proper lookup chains instead of hacky string manipulation. The correct flow is:

1. **Compiler frontend**: Resolves function call to `LpfxFn` → gets `float_impl` `BuiltinId` → generates call to `builtin_id.name()` (e.g., `__lpfx_saturate_vec3_f32`)
2. **Q32 Transform**: Sees `__lpfx_saturate_vec3_f32` in CLIF → looks up `BuiltinId` from name → finds corresponding `LpfxFn` → replaces with `q32_impl` `BuiltinId` → uses `builtin_id.name()` (e.g., `__lpfx_saturate_vec3_q32`)

Currently, the transform uses `map_testcase_to_builtin` which expects GLSL names without `_f32` suffixes, requiring string manipulation. This is fragile and incorrect.

## Architecture

### File Structure

```
lp-glsl/crates/lp-glsl-compiler/src/backend/builtins/
└── registry.rs                      # UPDATE: Add builtin_id_from_name function

lp-glsl/crates/lp-glsl-compiler/src/frontend/semantic/lpfx/
└── lpfx_fn_registry.rs              # UPDATE: Already has find_lpfx_fn_by_builtin_id

lp-glsl/crates/lp-glsl-compiler/src/backend/transform/q32/converters/
├── calls.rs                         # UPDATE: Use proper lookup chain for LPFX functions
└── math.rs                          # UPDATE: Remove LPFX entries from map_testcase_to_builtin

lp-glsl/apps/lp-builtin-gen/src/
└── main.rs                          # UPDATE: Generate builtin_id_from_name, remove LPFX from map_testcase_to_builtin
```

### Types and Functions

#### Builtin Registry (`registry.rs`)

```
BuiltinId::builtin_id_from_name(name: &str) -> Option<BuiltinId>
  # NEW: Reverse lookup of BuiltinId::name()
  # - Generated match statement mapping names to enum variants
  # - Returns None for unknown names
```

#### LPFX Function Registry (`lpfx_fn_registry.rs`)

```
find_lpfx_fn_by_builtin_id(builtin_id: BuiltinId) -> Option<&'static LpfxFn>
  # EXISTING: Finds LpfxFn that has this BuiltinId as float_impl or q32_impl
```

#### Q32 Transform (`calls.rs`)

```
convert_call(...) -> Result<(), GlslError>
  # UPDATE: For TestCase names:
  # 1. Try builtin_id_from_name(testcase_name)
  # 2. If Some(builtin_id), try find_lpfx_fn_by_builtin_id(builtin_id)
  # 3. If Some(lpfx_fn), extract q32_impl and use that
  # 4. If None, fall back to map_testcase_to_builtin for regular q32 functions
  # 5. If neither works, return error
```

#### Math Converter (`math.rs`)

```
map_testcase_to_builtin(testcase_name: &str, arg_count: usize) -> Option<BuiltinId>
  # UPDATE: Remove all LPFX function entries
  # - Keep only regular q32 functions (e.g., __lp_q32_sin)
  # - LPFX functions handled via proper lookup chain
```

#### Codegen Tool (`lp-builtin-gen/src/main.rs`)

```
generate_builtin_id_from_name(builtins: &[BuiltinInfo]) -> String
  # NEW: Generate builtin_id_from_name function
  # - Match on all builtin names
  # - Return corresponding BuiltinId variant

generate_map_testcase_to_builtin(...)
  # UPDATE: Filter out LPFX functions
  # - Only generate entries for regular q32 functions
```

## Design Decisions

### 1. Lookup Chain for LPFX Functions

Use the proper chain: `name` → `BuiltinId` → `LpfxFn` → `q32_impl` → `name`. This avoids string manipulation and ensures correctness.

### 2. Identification of LPFX Functions

Check if `builtin_id_from_name` returns `Some`, then check if `find_lpfx_fn_by_builtin_id` returns `Some`. This robustly identifies LPFX functions without string prefix checks.

### 3. Fallback for Regular Q32 Functions

If the builtin is not an LPFX function, fall back to `map_testcase_to_builtin` for regular q32 functions. This maintains backward compatibility for non-LPFX builtins.

### 4. Error Handling

If neither the LPFX lookup nor the regular q32 lookup works, return an error. This ensures we don't silently ignore unknown functions.

### 5. Codegen Tool Updates

- Generate `builtin_id_from_name` function in `registry.rs`
- Remove LPFX function entries from `map_testcase_to_builtin` generation
- This ensures consistency between generated code and manual code

## Implementation Notes

### Lookup Chain Flow

1. Transform sees `__lpfx_saturate_vec3_f32` in CLIF
2. Call `BuiltinId::builtin_id_from_name("__lpfx_saturate_vec3_f32")` → `Some(BuiltinId::LpfxSaturateVec3F32)`
3. Call `find_lpfx_fn_by_builtin_id(BuiltinId::LpfxSaturateVec3F32)` → `Some(lpfx_fn)` where `lpfx_fn.impls.float_impl == LpfxSaturateVec3F32`
4. Extract `lpfx_fn.impls.q32_impl` → `BuiltinId::LpfxSaturateVec3Q32`
5. Use `BuiltinId::LpfxSaturateVec3Q32.name()` → `"__lpfx_saturate_vec3_q32"`

### Generated Code Structure

The `builtin_id_from_name` function will be a match statement:

```rust
pub fn builtin_id_from_name(name: &str) -> Option<BuiltinId> {
    match name {
        "__lp_q32_acos" => Some(BuiltinId::LpQ32Acos),
        "__lp_q32_acosh" => Some(BuiltinId::LpQ32Acosh),
        // ... all builtin names ...
        "__lpfx_saturate_vec3_f32" => Some(BuiltinId::LpfxSaturateVec3F32),
        "__lpfx_saturate_vec3_q32" => Some(BuiltinId::LpfxSaturateVec3Q32),
        // ... all builtin names ...
        _ => None,
    }
}
```

### Testing Strategy

- Unit tests for `builtin_id_from_name` in `registry.rs`
- Unit tests for full lookup chain (f32 → q32) in `lpfx_fn_registry.rs`
- Integration tests via existing filetests to ensure end-to-end correctness

## Success Criteria

- No string manipulation in Q32 transform for LPFX functions
- Proper lookup chain used: `name` → `BuiltinId` → `LpfxFn` → `q32_impl` → `name`
- All existing tests pass
- Codegen tool generates `builtin_id_from_name` correctly
- `map_testcase_to_builtin` no longer contains LPFX function entries
