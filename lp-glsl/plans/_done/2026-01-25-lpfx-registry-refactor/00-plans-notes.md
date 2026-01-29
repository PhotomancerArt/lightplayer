# Plans Notes: LPFX Registry Refactor

## Context

The user has reorganized `/Users/yona/dev/photomancer/lp2025/lp-glsl/crates/lp-builtins/src/builtins` and renamed `lp_simplex` and `lp_noise` to use the `__lpfx` prefix. Functions should now be named `lpfx_*` in GLSL usage, with implementations named `__lpfx_<name>_<decimal-format>`.

Currently, `LpfxFnId` is a manual enum with hardcoded methods, and there are many hardcoded checks for lpfx function names throughout the codebase. Adding new functions is difficult.

The user wants a system that:
1. Can be easily codegen'd (though doesn't need to be codegen'd right now)
2. Eliminates hardcoded checks
3. Makes adding new functions easier
4. Uses a registry-based approach

The user has started sketching an idea in `/Users/yona/dev/photomancer/lp2025/lp-glsl/crates/lp-glsl-compiler/src/frontend/semantic/lpfx/` with:
- `lpfx_fn.rs` - Defines `LpfxFn` and `LpfxFnImpl` structures
- `lpfx_fn_registry.rs` - Registry structure (incomplete)
- `lpfx_sig.rs` - Helper functions for signatures
- `lpfx_fns.rs` - Placeholder for function data

## Questions

### Q1: Registry Structure and Ownership

**Question**: How should the registry be structured? Should it be:
- A static/const registry with all functions defined at compile time?
- A lazy-initialized singleton?
- A simple const array/vec that's searched?

**Context**: The current sketch shows a `LpfxFnRegistry` struct, but it's incomplete. We need to decide on the ownership model and how functions are stored.

**Suggested Answer**: Use a const static registry that's initialized at compile time. This is simple, fast, and works well for codegen. Functions can be stored in a `&'static [LpfxFn]` or similar structure.

**Answer**: Use const static registry. Functions will be defined in `lpfx_fns.rs` for easy codegen later.

### Q2: Function Identification

**Question**: How should functions be identified? Should we:
- Keep `LpfxFnId` as an enum but make it codegen-friendly?
- Use string-based lookups only?
- Use a hybrid approach (enum for type safety, string for lookups)?

**Context**: Currently `LpfxFnId` is used throughout the codebase for type-safe function identification. The enum approach provides compile-time safety but requires manual updates.

**Answer**: Remove all match statements about specific functions. Remove the need for type safety from `LpfxFnId` enum. Everything should be done dynamically based on `LpfxFn.glsl_sig`. Have helper functions that determine how to call them - just expand vectors into components and convert to int based on `DecimalFormat`. No other types supported (panic if unsupported). `LpfxFnId` can be just an index/identifier into the registry, or we might not need it at all if we can look up by name+args directly.

### Q3: Signature Handling

**Question**: How should function signatures be represented? Should we:
- Store full `FunctionSignature` objects in the registry?
- Store simplified signature info and construct `FunctionSignature` on demand?
- Use a different representation?

**Context**: The sketch shows `LpfxFn` containing a `glsl_sig: FunctionSignature`. Functions are NOT overloaded - they have distinct names like `lpfx_hash1`, `lpfx_hash2`, `lpfx_hash3`, etc.

**Answer**: Store full `FunctionSignature` objects in the registry. Since everything is dynamic and functions have unique names (no overloads), we can store complete signature info. Each function has exactly one `LpfxFn` entry.

### Q4: Implementation Mapping

**Question**: How should we map GLSL function calls to Rust implementations? Should we:
- Store all implementations (for different decimal formats) in each `LpfxFn`?
- Have separate registry entries for each format?
- Use a lookup function that constructs the implementation name?

**Context**: The sketch shows `LpfxFnImpl` with `decimal_format`, `builtin_module`, and `rust_fn_name`. Functions like `__lpfx_snoise1_q32` need to be mapped from `lpfx_snoise1(float, uint)`.

**Answer**: Store implementations in `LpfxFn` as a `Vec<LpfxFnImpl>`. When codegen needs a specific implementation, look up the appropriate `LpfxFnImpl` based on the decimal format. For functions that don't use decimal formats (like hash), have a single implementation with `decimal_format: None`.

### Q5: Backward Compatibility

**Question**: How should we handle the transition? Should we:
- Implement the new system alongside the old one and migrate gradually?
- Replace everything at once?
- Keep `LpfxFnId` enum but have it delegate to the registry?

**Context**: There are many places using `LpfxFnId` methods like `from_name_and_args`, `builtin_id`, `symbol_name`, etc. We need to maintain these APIs during transition.

**Answer**: Replace everything at once. Since we're removing match statements and making everything dynamic, we can replace the old system directly. Keep `LpfxFnId` as a simple identifier (maybe just an index or name) that can be looked up in the registry, but remove all the hardcoded methods. Update all call sites to use registry lookups instead.

### Q6: Codegen Readiness

**Question**: What should the codegen input format be? Should we:
- Use a simple Rust data structure that's easy to generate?
- Use a separate config file (TOML, JSON, etc.)?
- Use proc macros?

**Context**: The user wants it codegen-ready but doesn't need codegen right now.

**Answer**: Use a simple Rust data structure (const array/vec) that's easy to generate. Structure it so a codegen tool can easily produce it from a config file or builtin discovery. For now, manually maintain it, but make the structure codegen-friendly. The functions will be defined in `lpfx_fns.rs` as a const array - this file will be the output of codegen once we get there.

## Notes

- Current hardcoded checks are in:
  - `backend/transform/q32/converters/math.rs` - `map_testcase_to_builtin` function
  - `backend/builtins/registry.rs` - `BuiltinId::name()` and signature methods
  - `frontend/codegen/lp_lib_fns.rs` - Uses `LpfxFnId` methods
  - `frontend/semantic/type_check/inference.rs` - Checks for lpfx functions
  - `frontend/codegen/expr/function.rs` - Routes lpfx function calls
  - `apps/lp-builtin-gen/src/main.rs` - Generates mappings using `LpfxFnId::all()`

- The new system should eliminate all these hardcoded checks by centralizing function definitions in the registry.
