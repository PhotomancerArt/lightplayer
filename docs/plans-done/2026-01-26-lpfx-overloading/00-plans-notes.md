# Plan: Support for LPFX Function Overloading

## Questions

### Q1: How should overloaded functions be stored in the registry?

**Context:** Currently, `lpfx_fns()` returns `&'static [LpfxFn]` where each `LpfxFn` has a single
`glsl_sig`. The `find_lpfx_fn` function does a simple name lookup and returns the first match. This
doesn't support multiple signatures with the same name (overloading).

**Suggested Answer:** Change the registry structure to group overloads by name, similar to how
`FunctionRegistry` works. We could either:

- Option A: Change `lpfx_fns()` to return a `HashMap<String, Vec<LpfxFn>>`
- Option B: Keep the flat array but change `find_lpfx_fn` to return all matches, then add a new
  `find_lpfx_fn_overload` that does resolution
- Option C: Store overloads as multiple `LpfxFn` entries with the same name, and update lookup to
  find all matches and resolve

**Answer:** Option C - Keep the flat array structure, allow multiple `LpfxFn` entries with the same
name, and update lookup to find all matches and resolve based on argument types. This requires
minimal changes to the data structure.

### Q2: How should overload resolution work?

**Context:** GLSL supports function overloading based on argument types. The regular
`FunctionRegistry` does overload resolution with exact match first, then convertible match. For LPFX
functions, we need to match based on:

- Parameter count
- Parameter types (including vector component counts)
- Return type (for disambiguation if needed)

**Suggested Answer:** Implement overload resolution similar to `FunctionRegistry::lookup_function`:

1. Find all functions with matching name
2. Try exact type match first
3. If no exact match, try with implicit conversions (if any apply)
4. Return error if ambiguous or no match found

**Answer:** Only match on parameters, not return type. Return type should not be ambiguous - if two
overloads have the same parameter types but different return types, that would be an error. For now,
use exact type matches only (no implicit conversions) for safety with builtin functions.

### Q3: How should the codegen tool handle overloaded functions?

**Context:** Currently, `lp-glsl-builtin-gen-app` groups functions by GLSL name in
`group_functions_by_name`, but then only uses the first function's signature. If we have
`__lpfx_hsv2rgb_q32` with signature `vec3 lpfx_hsv2rgb(vec3)` and `__lpfx_hsv2rgb_vec4_q32` with
signature `vec4 lpfx_hsv2rgb(vec4)`, both should be registered.

**Suggested Answer:** Update the codegen to:

1. Group functions by GLSL name (already done)
2. For each group, create multiple `LpfxFn` entries - one per unique signature
3. Each entry should have the correct `BuiltinId` mapped to its signature

**Answer:** Yes, validate that overloads have distinct parameter signatures to avoid ambiguity.
Update codegen to create multiple `LpfxFn` entries - one per unique signature, each with the correct
`BuiltinId` mapped to its signature.

### Q4: How should the codegen (compiler) handle overload resolution?

**Context:** The `emit_lp_lib_fn_call` function currently calls `find_lpfx_fn(name)` which returns a
single function. We need to pass argument types and resolve the overload. Both `emit_lp_lib_fn_call`
functions have access to `args: Vec<(Vec<Value>, Type)>`, and `check_lpfx_fn_call` already receives
`arg_types: &[Type]`.

**Answer:**

1. Change `find_lpfx_fn` to require argument types:
   `find_lpfx_fn(name: &str, arg_types: &[Type]) -> Option<&'static LpfxFn>`
2. Implement overload resolution logic: find all functions with matching name, then match on
   parameter types (exact match only)
3. Update `emit_lp_lib_fn_call` to extract types from args and pass to `find_lpfx_fn`
4. Update `check_lpfx_fn_call` to use the new `find_lpfx_fn` and extract return type from result
5. All call sites already have access to argument types, so this is a clean change

### Q5: What about backward compatibility?

**Context:** All current call sites of `find_lpfx_fn` have access to argument types, so we can
change the signature directly.

**Answer:**

- Change `find_lpfx_fn` signature to require `arg_types` - all call sites can provide this
- No need for backward compatibility wrapper since all usages are internal and can be updated
  together

### Q6: How should we handle the hsv2rgb issue specifically?

**Context:** Currently, `lpfx_hsv2rgb` has two overloads:

- `vec3 lpfx_hsv2rgb(vec3 hsv)` -> `__lpfx_hsv2rgb_q32` (4 args: result_ptr + 3 components)
- `vec4 lpfx_hsv2rgb(vec4 hsv)` -> `__lpfx_hsv2rgb_vec4_q32` (5 args: result_ptr + 4 components)

But the registry only has one entry pointing to the vec4 version, causing the error "Expected 5
argument(s) for math function '__lpfx_hsv2rgb', got 4".

**Suggested Answer:**

1. Register both overloads in the registry
2. Update lookup to resolve based on argument types
3. The codegen will then call the correct builtin based on the resolved signature

**Answer:** Once overload support is implemented, both overloads will be registered and resolved
correctly. The existing filetests in `lp_hsv2rgb.glsl` already cover testing both vec3 and vec4
versions, so no additional test case needed.
