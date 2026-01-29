# Design: Support for LPFX Function Overloading

## Overview

Add support for function overloading in LPFX functions, allowing multiple implementations with the same GLSL name but different parameter signatures (e.g., `lpfx_hsv2rgb(vec3)` and `lpfx_hsv2rgb(vec4)`). This enables porting lygia functions that use overloading.

## Architecture

### File Structure

```
lp-glsl/crates/lp-glsl-compiler/src/frontend/semantic/lpfx/
├── lpfx_fn.rs                    # UPDATE: No changes needed
├── lpfx_fns.rs                   # UPDATE: Generated code will have multiple entries per name
├── lpfx_fn_registry.rs           # UPDATE: Add overload resolution to find_lpfx_fn
└── lpfx_sig.rs                   # No changes

lp-glsl/crates/lp-glsl-compiler/src/frontend/codegen/
├── lpfx_fns.rs                   # UPDATE: Extract arg types and pass to find_lpfx_fn
└── lp_lib_fns.rs                 # UPDATE: Extract arg types and pass to find_lpfx_fn

lp-glsl/apps/lp-builtin-gen/src/lpfx/
├── generate.rs                   # UPDATE: Generate multiple LpfxFn entries per unique signature
└── validate.rs                   # UPDATE: Validate distinct signatures for overloads
```

### Types and Functions

#### Registry Lookup (`lpfx_fn_registry.rs`)

```
find_lpfx_fn(name: &str, arg_types: &[Type]) -> Option<&'static LpfxFn>
  # UPDATE: Now requires arg_types, does overload resolution
  # - Finds all functions with matching name
  # - Matches on parameter types (exact match only)
  # - Returns first match or None if ambiguous/no match

check_lpfx_fn_call(name: &str, arg_types: &[Type]) -> Result<Type, String>
  # UPDATE: Uses find_lpfx_fn with arg_types, extracts return type
```

#### Codegen (`lpfx_fns.rs`, `lp_lib_fns.rs`)

```
emit_lp_lib_fn_call(name: &str, args: Vec<(Vec<Value>, Type)>) -> Result<...>
  # UPDATE: Extract arg_types from args, pass to find_lpfx_fn
```

#### Codegen Tool (`lp-builtin-gen/src/lpfx/generate.rs`)

```
generate_lpfx_fns(parsed_functions: &[ParsedLpfxFunction]) -> String
  # UPDATE: Generate multiple LpfxFn entries for each unique signature
  # - Group by GLSL name (already done)
  # - For each group, create one LpfxFn per unique signature
  # - Each entry maps to correct BuiltinId for its signature

validate_overloads(functions: &[ParsedLpfxFunction]) -> Result<(), Error>
  # NEW: Validate that overloads have distinct parameter signatures
```

## Design Decisions

### 1. Registry Structure

Keep the flat array structure (`&'static [LpfxFn]`), allowing multiple entries with the same GLSL name. This requires minimal changes and maintains the current memory layout.

### 2. Overload Resolution

- Match only on parameter types (not return type)
- Exact type matching only (no implicit conversions)
- Return error if ambiguous (multiple exact matches) or no match found

### 3. Lookup Function Signature

Change `find_lpfx_fn` to require `arg_types` parameter since overload resolution always needs argument types. All current call sites have access to argument types, so this is a clean change.

### 4. Codegen Tool Updates

- Generate multiple `LpfxFn` entries for functions with the same GLSL name but different signatures
- Validate that overloads have distinct parameter signatures
- Each entry correctly maps to its corresponding `BuiltinId`

### 5. Backward Compatibility

No backward compatibility needed - all call sites are internal and can be updated together. The signature change is breaking but acceptable since it's compiler-internal code.

## Implementation Notes

### Overload Resolution Algorithm

1. Find all functions in registry with matching GLSL name
2. Filter to functions with matching parameter count
3. For each candidate, check exact type match for all parameters
4. Return first exact match, or None if ambiguous/no match

### Parameter Type Matching

- Scalar types: exact match required
- Vector types: exact match required (including component count)
- No implicit conversions (int to float, vec2 to vec3, etc.)

### Error Handling

- No match found: return `None` from `find_lpfx_fn`, codegen will emit "Unknown LPFX function" error
- Ambiguous match: return `None` from `find_lpfx_fn`, codegen will emit "Ambiguous overload" error
- Type mismatch: handled by `check_lpfx_fn_call` which validates after resolution

## Success Criteria

- Multiple overloads of the same function name can be registered
- Overload resolution correctly selects implementation based on argument types
- `lpfx_hsv2rgb(vec3)` and `lpfx_hsv2rgb(vec4)` both work correctly
- Existing filetests pass
- Codegen tool validates distinct signatures for overloads
- All code compiles without warnings
- Code formatted with `cargo +nightly fmt`
