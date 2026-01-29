# Design: LPFX Registry Refactor

## Overview

Refactor the LPFX function system to use a registry-based approach that eliminates hardcoded checks and makes adding new functions trivial. The system will be fully dynamic, driven by function signatures stored in the registry.

## File Structure

```
frontend/semantic/lpfx/
├── mod.rs                      # UPDATE: Re-exports and module organization
├── lpfx_fn.rs                  # UPDATE: LpfxFn and LpfxFnImpl structures
├── lpfx_fns.rs                 # UPDATE: Const array of all LPFX functions (codegen output)
├── lpfx_fn_registry.rs         # UPDATE: Registry lookup functions and utilities
└── lpfx_sig.rs                 # UPDATE: Helper functions for signature conversion and calling
```

## Type Summary

### Core Types (`lpfx_fn.rs`)

```
LpfxFn - # UPDATE: Function definition
├── glsl_sig: FunctionSignature - # GLSL signature (name, params, return type)
└── impls: Vec<LpfxFnImpl> - # Implementations for different decimal formats

LpfxFnImpl - # UPDATE: Implementation details
├── decimal_format: Option<DecimalFormat> - # Format-specific implementation (None = format-agnostic)
├── builtin_module: &'static str - # Module path in lp_builtins
└── rust_fn_name: &'static str - # Rust function name (e.g., "__lpfx_hash_1")
```

### Registry (`lpfx_fns.rs`)

```
LPFX_FNS: &'static [LpfxFn] - # NEW: Const array of all LPFX functions
```

### Registry API (`lpfx_fn_registry.rs`)

```
is_lpfx_fn(name: &str) -> bool - # NEW: Check if name is an LPFX function
find_lpfx_fn(name: &str) -> Option<&LpfxFn> - # NEW: Lookup by GLSL name
check_lpfx_fn_call(name: &str, arg_types: &[Type]) -> Result<Type, String> - # NEW: Validate call and return return type
get_impl_for_format(fn: &LpfxFn, format: DecimalFormat) -> Option<&LpfxFnImpl> - # NEW: Get implementation for decimal format
```

### Signature Helpers (`lpfx_sig.rs`)

```
expand_vector_args(param_types: &[Type], values: &[Value]) -> Vec<Value> - # NEW: Expand vectors to components
convert_to_cranelift_types(param_types: &[Type], format: DecimalFormat) -> Vec<CraneliftType> - # NEW: Convert GLSL types to Cranelift types based on format
build_call_signature(fn: &LpfxFn, impl: &LpfxFnImpl, format: DecimalFormat) -> Signature - # NEW: Build Cranelift signature dynamically
```

## Design Decisions

### 1. No Function Overloading

Functions have unique names (e.g., `lpfx_hash1`, `lpfx_hash2`, `lpfx_hash3`). No overload resolution needed - direct name lookup.

### 2. Dynamic Signature Handling

All signature conversion and type checking is done dynamically based on `FunctionSignature` stored in the registry. No match statements on specific function names.

### 3. Type Conversion Rules

- Vectors are expanded to their components (vec2 → 2 scalars, vec3 → 3 scalars)
- Types are converted to int based on `DecimalFormat`:
  - Float → i32 (q32 representation)
  - UInt → i32 (Cranelift representation)
  - Int → i32
- Unsupported types panic (no other types allowed)

### 4. Implementation Selection

Each `LpfxFn` contains a `Vec<LpfxFnImpl>`. When codegen needs an implementation:
1. Look up the function by name
2. Find the appropriate `LpfxFnImpl` based on `DecimalFormat`
3. Use `rust_fn_name` and `builtin_module` to construct the call

### 5. Registry Storage

Functions are stored in a const array `LPFX_FNS` in `lpfx_fns.rs`. This file will be codegen output in the future, but for now is manually maintained.

### 6. Backward Compatibility

Replace `LpfxFnId` enum entirely. All lookups are by name. If we need an identifier, use the function name string or array index.

## Migration Strategy

1. Define all functions in `lpfx_fns.rs` as const array
2. Implement registry lookup functions in `lpfx_fn_registry.rs`
3. Implement signature helpers in `lpfx_sig.rs`
4. Update all call sites to use registry lookups:
   - `frontend/semantic/type_check/inference.rs` - Use `check_lpfx_fn_call`
   - `frontend/codegen/expr/function.rs` - Use `is_lpfx_fn` and `find_lpfx_fn`
   - `frontend/codegen/lp_lib_fns.rs` - Use registry for all function info
   - `backend/transform/q32/converters/math.rs` - Use registry for testcase mapping
   - `backend/builtins/registry.rs` - Use registry for builtin name mapping
   - `apps/lp-builtin-gen/src/main.rs` - Use registry instead of `LpfxFnId::all()`
5. Remove `LpfxFnId` enum and all its methods
6. Remove all hardcoded match statements

## Success Criteria

- All hardcoded function name checks removed
- Adding a new function requires only adding an entry to `lpfx_fns.rs`
- No match statements on specific function names
- All type conversion and signature handling is dynamic
- Code compiles and tests pass
- Code formatted with `cargo +nightly fmt`
