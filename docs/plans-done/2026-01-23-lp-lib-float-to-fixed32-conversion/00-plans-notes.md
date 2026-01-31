# Questions for LP Library Float-to-Q32 Conversion Plan

## Context

We implemented LP library functions (`lpfx_snoise1`, `lpfx_snoise2`, `lpfx_snoise3`) but missed a
critical architectural detail:

- **LP library functions are semantically float functions** - they operate on float types in GLSL
- **Eventually we'll have float implementations** - when native float support is added
- **Currently we only support q32** - so we have `__lp_q32_lpfx_snoise3` implementations
- **The compiler should generate float calls** - which the q32 transform then converts

This matches the pattern used for `sin`/`cos`:

- Codegen generates TestCase calls to `"sinf"` or `"__lp_sin"` (float semantics)
- Q32 transform converts `"sinf"` → `__lp_q32_sin` via `map_testcase_to_builtin()`

Currently, `lp_lib` functions bypass this pattern:

- Codegen directly calls builtins via `get_builtin_func_ref()`
- This skips the float → q32 conversion step

## Questions

### Q1: Naming Convention for Float LP Library Functions

**Context**: We need to decide what the "float" version of these functions should be called in the
compiler's intermediate representation.

**Question**: What should the TestCase/function name be for float LP library functions?

**Options**:

- Option A: `__lp_float_lpfx_snoise3` (explicit float prefix)
- Option B: `__lpfx_snoise3` (no float prefix, just the base name)
- Option C: `lpfx_snoise3` (user-facing name, no prefix)
- Option D: Something else?

**Suggested Answer**: Option B (`__lpfx_snoise3`) - matches the pattern used for `sin`/`cos` where
`"sinf"` maps to `"__lp_sin"` in the mapping table. The float nature is implicit since we're in
float codegen before the transform.

**ANSWERED**: Option B (`__lpfx_snoise3`) - confirmed by user. This matches
`LpLibFn::Simplex3.symbol_name()` and the existing mapping table entries.

### Q2: Codegen Strategy for LP Library Functions

**Context**: Currently `emit_lp_lib_fn_call()` directly gets builtin function references. We need to
change it to emit TestCase calls instead.

**Question**: How should codegen emit LP library function calls?

**Options**:

- Option A: Use `LpLibFn::symbol_name()` to get `"__lpfx_snoise3"`, emit TestCase call similar to
  `get_math_libcall()` pattern
- Option B: Create new helper similar to `get_math_libcall()` but for LP library functions
- Option C: Something else?

**Suggested Answer**: Option A - Use `LpLibFn::symbol_name()` to get TestCase name, emit TestCase
call using same pattern as `get_math_libcall()`. Need to handle different argument counts and vector
argument flattening.

**ANSWERED**: Option A - confirmed by user. Use `LpLibFn::symbol_name()` and emit TestCase calls
following the `get_math_libcall()` pattern.

### Q3: BuiltinId Mapping Strategy

**Context**: The registry has been regenerated. `BuiltinId::LpSimplex3.name()` should now return
`"__lp_q32_lpfx_snoise3"` (the actual function name). The mapping table maps `"__lpfx_snoise3"` (
semantic/float name from `LpLibFn::symbol_name()`) → `BuiltinId::LpSimplex3`.

**Question**: How should the transform convert TestCase calls?

**Suggested Answer**: The transform should:

1. Map `"__lpfx_snoise3"` → `BuiltinId::LpSimplex3` via `map_testcase_to_builtin()`
2. Use `BuiltinId::LpSimplex3.name()` to get `"__lp_q32_lpfx_snoise3"`
3. Look up `"__lp_q32_lpfx_snoise3"` in `func_id_map` to get the FuncId
4. Create call to that function

This matches how `sin` works: `"sinf"` → `BuiltinId::Q32Sin` → `"__lp_q32_sin"`.

**Note**: The generator should be driven by `LpLibFn` enum, not prefix matching. It should:

- Read `LpLibFn` enum to know what LP library functions exist
- Match discovered function names to `LpLibFn::symbol_name()` values
- Use `LpLibFn::builtin_id()` to determine the `BuiltinId` variant
- Generate registry entries based on actual function names (which may be `__lp_q32_lpfx_snoise3`)

### Q4: Hash Function Handling

**Context**: Hash functions (`lpfx_hash`) are not float-specific - they operate on integers. They're
correctly in `builtins/shared/`.

**Question**: Should hash functions follow the same pattern, or stay as-is?

**Suggested Answer**: `LpLibFn` should be the source of truth for determining:

1. Whether a function needs q32 mapping (simplex functions do, hash functions don't)
2. What the q32 implementation name is (e.g., `__lp_q32_lpfx_snoise3` for simplex functions)

Hash functions don't need conversion, so they can stay as direct builtin calls. But `LpLibFn` should
have methods to determine this programmatically.

**ANSWERED**: Option A - Keep hash functions as direct builtin calls, but extend `LpLibFn` with
methods to determine if a function needs q32 mapping and what the mapped name is. This keeps the
source of truth in `LpLibFn`.

**Implementation Note**: Add methods to `LpLibFn`:

- `q32_name(&self) -> Option<&'static str>` - returns `Some("__lp_q32_lpfx_snoise3")` for simplex
  functions, `None` for hash functions (single source of truth)
- `needs_q32_mapping(&self) -> bool` - delegates to `q32_name().is_some()` to keep a single source
  of truth

This allows codegen to check if a function needs TestCase conversion or can be called directly.

### Q5: Generator Fix

**Context**: The generator currently uses prefix matching to discover functions. It should use
`LpLibFn` as the source of truth.

**Question**: How should the generator discover and map functions?

**Suggested Answer**: Generator should:

1. Read `LpLibFn` enum to know what functions exist
2. For each variant, look for matching function name (e.g., `__lp_q32_lpfx_snoise3` for simplex,
   `__lpfx_hash_*` for hash)
3. Use `LpLibFn::builtin_id()` to determine `BuiltinId` variant name (e.g., `LpSimplex3`, not
   `Q32LpSimplex3`)
4. Map `BuiltinId::LpSimplex3.name()` to actual function name found

**ANSWERED**: Confirmed by user. Generator should use `LpLibFn` enum as source of truth and match
discovered functions to expected names.

### Q6: Backward Compatibility

**Context**: The current implementation works, but uses the wrong pattern. Changing it might break
existing code.

**Question**: Do we need to maintain backward compatibility, or can we change the pattern?

**Suggested Answer**: Since this is a recent implementation and the pattern is wrong, we should fix
it now. No need for backward compatibility - the correct pattern is more important.

**ANSWERED**: No backward compatibility needed - fix the pattern now.

### Q6: Float Implementation Placeholder

**Context**: We don't have float implementations yet, but the architecture should assume they'll
exist.

**Question**: Should we create placeholder float implementations, or just assume they'll be added
later?

**Suggested Answer**: No placeholder needed. The architecture should support float implementations,
but we don't need to implement them now. When float support is added, we'll add
`__lp_float_lpfx_snoise3` (or similar) implementations, and the codegen will generate calls to those
instead of the q32 versions.

Actually, wait - if we're generating TestCase calls to `__lpfx_snoise3`, and there's no float
implementation yet, what happens? The transform will convert `__lpfx_snoise3` →
`__lp_q32_lpfx_snoise3`, which exists. So the current flow is:

- Codegen: `lpfx_snoise3(...)` → TestCase call to `__lpfx_snoise3`
- Transform: `__lpfx_snoise3` → `__lp_q32_lpfx_snoise3` (via mapping)
- Runtime: Calls `__lp_q32_lpfx_snoise3`

When float support is added:

- Codegen: `lpfx_snoise3(...)` → TestCase call to `__lpfx_snoise3` (same)
- Transform: If float target, keep as `__lp_float_lpfx_snoise3` (or similar)
- Runtime: Calls `__lp_float_lpfx_snoise3`

So the TestCase name `__lpfx_snoise3` represents the "semantic" function, and the transform decides
which implementation to use based on target.

## Notes

### Current State Analysis

1. **Registry Mismatch**:
    - Registry has `Q32LpSimplex1/2/3` (generated from prefix matching)
    - `lp_lib_fns.rs` expects `LpSimplex1/2/3` (semantic names)
    - Generator needs to use `LpLibFn` enum as source of truth

2. **Codegen** (`lp_lib_fns.rs`):
    - Currently calls `get_builtin_func_ref(builtin_id)` directly
    - This bypasses the TestCase → q32 conversion pattern
    - Should emit TestCase calls using `LpLibFn::symbol_name()` (e.g., `"__lpfx_snoise3"`)

3. **Transform** (`calls.rs`):
    - `map_testcase_to_builtin()` should map `"__lpfx_snoise3"` → `BuiltinId::LpSimplex3`
    - `BuiltinId::LpSimplex3.name()` should return `"__lp_q32_lpfx_snoise3"` (actual function)
    - Transform looks up `"__lp_q32_lpfx_snoise3"` in `func_id_map`

4. **Hash Functions**:
    - In `builtins/shared/` (correct - not float-specific)
    - Use `__lpfx_hash_*` naming (no `q32` prefix)
    - Should continue to work, but verify codegen path

### Architecture

**Source of Truth**: `LpLibFn` enum in `lp_lib_fns.rs` defines:

- Semantic function names (`lpfx_snoise3`)
- TestCase names (`__lpfx_snoise3`) via `symbol_name()`
- BuiltinId mapping (`LpSimplex3`) via `builtin_id()`

**Generator Should**:

- Read `LpLibFn` enum to know what functions exist
- Match discovered function names to `LpLibFn::symbol_name()` values
- Generate `BuiltinId` variants matching `LpLibFn::builtin_id()` (e.g., `LpSimplex3`, not
  `Q32LpSimplex3`)
- Map `BuiltinId::LpSimplex3.name()` to actual function name (`__lp_q32_lpfx_snoise3`)

**Flow**:

1. Codegen: `lpfx_snoise3(...)` → TestCase call to `LpLibFn::Simplex3.symbol_name()` =
   `"__lpfx_snoise3"`
2. Transform: `"__lpfx_snoise3"` → `BuiltinId::LpSimplex3` (via `map_testcase_to_builtin()`)
3. Transform: `BuiltinId::LpSimplex3.name()` → `"__lp_q32_lpfx_snoise3"`
4. Transform: Look up `"__lp_q32_lpfx_snoise3"` in `func_id_map` → create call
5. Runtime: Calls `__lp_q32_lpfx_snoise3`

### Implementation Strategy

1. **Fix Generator** - Update `lp-glsl-builtin-gen-app` to use `LpLibFn` enum as source of truth:
    - Read `LpLibFn` enum from `lp_lib_fns.rs`
    - Match discovered functions to `LpLibFn::symbol_name()` values
    - Use `LpLibFn::builtin_id()` to determine `BuiltinId` variant
    - Generate registry with actual function names (e.g., `__lp_q32_lpfx_snoise3`)

2. **Fix Codegen** - Change `emit_lp_lib_fn_call()` to emit TestCase calls:
    - Use `LpLibFn::symbol_name()` to get TestCase name (e.g., `"__lpfx_snoise3"`)
    - Emit TestCase call instead of direct builtin call
    - Let q32 transform handle conversion

3. **Verify Transform** - Ensure `map_testcase_to_builtin()` correctly maps:
    - `"__lpfx_snoise3"` → `BuiltinId::LpSimplex3`
    - Transform uses `BuiltinId::LpSimplex3.name()` → `"__lp_q32_lpfx_snoise3"`
    - Looks up `"__lp_q32_lpfx_snoise3"` in `func_id_map`

4. **Test** - Verify end-to-end flow:
    - Codegen emits TestCase call to `"__lpfx_snoise3"`
    - Transform converts to `__lp_q32_lpfx_snoise3` call
    - Runtime calls correct function

5. **Hash Functions** - Verify hash functions still work (they're not float-specific, so may not
   need conversion)
