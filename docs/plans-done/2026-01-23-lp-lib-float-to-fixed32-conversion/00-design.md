# Design: LP Library Float-to-Q32 Conversion

## Overview

Fix LP library functions (`lpfx_snoise1/2/3`, `lpfx_hash`) to follow the correct float→q32
conversion pattern. Currently, codegen directly calls builtins, bypassing the transform. Functions
should emit TestCase calls that the q32 transform converts, matching the pattern used for `sin`/
`cos`.

## File Structure

```
lp-glsl/lp-glsl-compiler/src/
├── frontend/
│   ├── semantic/
│   │   └── lp_lib_fns.rs              # UPDATE: Add needs_q32_mapping() and q32_name() methods
│   └── codegen/
│       └── lp_lib_fns.rs              # UPDATE: Change emit_lp_lib_fn_call() to emit TestCase calls
│
├── backend/
│   ├── builtins/
│   │   └── registry.rs                # UPDATE: Regenerate with correct BuiltinId names (LpSimplex3, not Q32LpSimplex3)
│   └── transform/
│       └── q32/
│           └── converters/
│               ├── calls.rs           # VERIFY: Already handles TestCase→builtin conversion correctly
│               └── math.rs            # VERIFY: map_testcase_to_builtin() already has correct mappings

lp-glsl/lp-glsl-builtin-gen-app/
└── src/
    └── main.rs                        # UPDATE: Use LpLibFn enum as source of truth instead of prefix matching
```

## Types Summary

### LpLibFn Extensions (`frontend/semantic/lp_lib_fns.rs`)

```
LpLibFn - # UPDATE: Add new methods
├── needs_q32_mapping(&self) -> bool - # NEW: Returns true for simplex functions, false for hash
└── q32_name(&self) -> Option<&'static str> - # NEW: Returns Some("__lp_q32_lpfx_snoise3") or None

Existing methods (unchanged):
├── symbol_name(&self) -> &'static str - # Returns "__lpfx_snoise3" (TestCase name)
├── builtin_id(&self) -> BuiltinId - # Returns BuiltinId::LpSimplex3
└── user_name(&self) -> &'static str - # Returns "lpfx_snoise3"
```

### Codegen Changes (`frontend/codegen/lp_lib_fns.rs`)

```
emit_lp_lib_fn_call() - # UPDATE: Change implementation
├── OLD: Direct builtin call via get_builtin_func_ref()
└── NEW: Emit TestCase call using LpLibFn::symbol_name()
    ├── Check if needs_q32_mapping() -> true
    ├── Get TestCase name from symbol_name() (e.g., "__lpfx_snoise3")
    ├── Flatten vector arguments
    ├── Emit TestCase call (similar to get_math_libcall() pattern)
    └── Let q32 transform handle conversion
```

### Generator Changes (`apps/lp-glsl-builtin-gen-app/src/main.rs`)

```
discover_builtins() - # UPDATE: Use LpLibFn enum as source of truth
├── OLD: Prefix matching (__lp_q32_, __lpfx_hash_, __lpfx_snoise)
└── NEW: Read LpLibFn enum
    ├── For each LpLibFn variant:
    │   ├── Use LpLibFn::builtin_id() to get BuiltinId name (e.g., LpSimplex3)
    │   ├── Use LpLibFn::q32_name() or symbol_name() to find actual function
    │   └── Match discovered function names to expected names
    └── Generate registry with correct BuiltinId names

extract_builtin() - # UPDATE: Match functions to LpLibFn variants
└── Match function names to LpLibFn::q32_name() or symbol_name() values
```

### Registry Changes (`backend/builtins/registry.rs`)

```
BuiltinId - # UPDATE: Regenerate with correct names
├── OLD: Q32LpSimplex1, Q32LpSimplex2, Q32LpSimplex3
└── NEW: LpSimplex1, LpSimplex2, LpSimplex3 (matches LpLibFn::builtin_id())

BuiltinId::LpSimplex3.name() - # UPDATE: Return actual function name
└── Returns "__lp_q32_lpfx_snoise3" (actual implementation)
```

## Architecture Flow

### Current (Incorrect) Flow

```
1. GLSL: lpfx_snoise3(vec3(1.0, 2.0, 3.0), 123u)
2. Codegen: emit_lp_lib_fn_call() → get_builtin_func_ref(BuiltinId::LpSimplex3)
3. Direct call to __lp_q32_lpfx_snoise3 (bypasses transform)
```

### New (Correct) Flow

```
1. GLSL: lpfx_snoise3(vec3(1.0, 2.0, 3.0), 123u)
2. Codegen: emit_lp_lib_fn_call()
   ├── Check needs_q32_mapping() → true
   ├── Get symbol_name() → "__lpfx_snoise3"
   ├── Flatten vec3 → (i32, i32, i32, u32)
   └── Emit TestCase call to "__lpfx_snoise3"
3. Q32 Transform: convert_call()
   ├── Detect TestCase call to "__lpfx_snoise3"
   ├── map_testcase_to_builtin("__lpfx_snoise3") → BuiltinId::LpSimplex3
   ├── BuiltinId::LpSimplex3.name() → "__lp_q32_lpfx_snoise3"
   ├── Look up "__lp_q32_lpfx_snoise3" in func_id_map
   └── Create call to __lp_q32_lpfx_snoise3
4. Runtime: Calls __lp_q32_lpfx_snoise3
```

### Hash Functions Flow (Unchanged)

```
1. GLSL: lpfx_hash(42u, 123u)
2. Codegen: emit_lp_lib_fn_call()
   ├── Check needs_q32_mapping() → false
   └── Direct builtin call to __lpfx_hash_1 (no TestCase conversion needed)
3. Runtime: Calls __lpfx_hash_1
```

## Design Decisions

### 1. LpLibFn as Source of Truth

`LpLibFn` enum is the single source of truth for:

- Which functions exist
- Whether they need q32 mapping
- What their TestCase names are
- What their BuiltinId variants are
- What their q32 implementation names are

This ensures consistency across codegen, transform, and generator.

### 2. TestCase Names Represent Semantic Functions

TestCase names like `"__lpfx_snoise3"` represent the semantic function (float version), not the
implementation. The transform decides which implementation to use based on the target:

- Q32 target: `"__lpfx_snoise3"` → `__lp_q32_lpfx_snoise3`
- Float target (future): `"__lpfx_snoise3"` → `__lp_float_lpfx_snoise3` (or similar)

### 3. Hash Functions Don't Need Conversion

Hash functions operate on integers (`u32`), not floats, so they don't need float→q32 conversion.
They can be called directly as builtins. The `needs_q32_mapping()` method returns `false` for hash
functions.

### 4. Generator Uses LpLibFn Enum

The generator reads `LpLibFn` enum to know what functions should exist, then matches discovered
function names to expected names. This ensures the registry matches what `lp_lib_fns.rs` expects.

### 5. Consistent with sin/cos Pattern

This matches the existing pattern for `sin`/`cos`:

- Codegen emits TestCase call to `"sinf"` or `"__lp_sin"`
- Transform converts to `__lp_q32_sin` via `map_testcase_to_builtin()`
- Same pattern applies to LP library functions

## Implementation Details

### LpLibFn Method Implementations

```rust
impl LpLibFn {
    /// Get the q32 implementation name, if this function needs mapping
    pub fn q32_name(&self) -> Option<&'static str> {
        match self {
            LpLibFn::Simplex1 => Some("__lp_q32_lpfx_snoise1"),
            LpLibFn::Simplex2 => Some("__lp_q32_lpfx_snoise2"),
            LpLibFn::Simplex3 => Some("__lp_q32_lpfx_snoise3"),

            // Hash functions don't have q32 versions
            LpLibFn::Hash1 => None,
            LpLibFn::Hash2 => None,
            LpLibFn::Hash3 => None,

            // Note: no catch-all case, so that we can detect missing q32 versions
        }
    }

    /// Check if this function needs q32 mapping (float→q32 conversion)
    /// Delegates to q32_name() to keep a single source of truth
    pub fn needs_q32_mapping(&self) -> bool {
        self.q32_name().is_some()
    }
}
```

### Codegen TestCase Call Pattern

```rust
// Similar to get_math_libcall() but for LP library functions
fn get_lp_lib_testcase_call(&mut self, lp_fn: LpLibFn, arg_count: usize) -> Result<FuncRef, GlslError> {
    let testcase_name = lp_fn.symbol_name(); // e.g., "__lpfx_snoise3"

    // Create signature based on argument count
    let mut sig = Signature::new(CallConv::SystemV);
    for _ in 0..arg_count {
        sig.params.push(AbiParam::new(types::F32)); // Float before transform
    }
    sig.returns.push(AbiParam::new(types::F32));

    // Create TestCase name
    let sig_ref = self.builder.func.import_signature(sig);
    let ext_name = ExternalName::testcase(testcase_name.as_bytes());
    let ext_func = ExtFuncData {
        name: ext_name,
        signature: sig_ref,
        colocated: false,
    };
    Ok(self.builder.func.import_function(ext_func))
}
```

### Generator Function Discovery

```rust
// Pseudo-code for generator changes
fn discover_lp_lib_functions() -> Vec<BuiltinInfo> {
    let mut builtins = Vec::new();

    // Read LpLibFn enum variants
    for lp_fn in [LpLibFn::Simplex1, LpLibFn::Simplex2, LpLibFn::Simplex3, ...] {
        // Find matching function name
        let expected_name = lp_fn.q32_name().unwrap_or_else(|| lp_fn.symbol_name());

        // Discover function with matching name
        if let Some(func) = find_function_by_name(expected_name) {
            builtins.push(BuiltinInfo {
                enum_variant: lp_fn.builtin_id().to_string(), // "LpSimplex3"
                symbol_name: expected_name, // "__lp_q32_lpfx_snoise3"
                function_name: func.name,
                param_count: func.param_count,
            });
        }
    }

    builtins
}
```

## Verification Points

1. **Registry**: `BuiltinId::LpSimplex3.name()` returns `"__lp_q32_lpfx_snoise3"`
2. **Mapping**: `map_testcase_to_builtin("__lpfx_snoise3")` returns `BuiltinId::LpSimplex3`
3. **Codegen**: `emit_lp_lib_fn_call()` emits TestCase calls for simplex functions
4. **Transform**: `convert_call()` correctly converts TestCase calls to builtin calls
5. **Hash**: Hash functions continue to work as direct builtin calls
