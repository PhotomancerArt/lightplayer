# Phase 1: Infrastructure Setup

## Goal

Set up the infrastructure needed for q32 math functions: test helpers, registry entries, and
transform mapping.

## Tasks

### 1.1 Create Test Helper Functions

Create shared test utilities in `lp-glsl-builtins/src/q32/test_helpers.rs`:

- Abstract the `float_to_fixed` and `fixed_to_float` helpers from `sqrt.rs`
- Create a test helper function that accepts:
    - Function to test
    - Array of `(input, expected_output)` pairs
    - Tolerance value
- Helper should convert inputs/outputs, call function, and assert with tolerance

### 1.2 Add Builtin Registry Entries

In `lp-glsl-compiler/src/backend/builtins/registry.rs`:

- Add `Q32Sin` and `Q32Cos` to `BuiltinId` enum
- Add symbol names: `"__lp_q32_sin"` and `"__lp_q32_cos"`
- Add signatures: `(i32) -> i32` for both
- Add to `all()` array
- Add function pointer mappings in `get_function_pointer()`

### 1.3 Create Mapping Table

In `lp-glsl-compiler/src/backend/transform/q32/converters/math.rs`:

- Create mapping table: `TestCase name -> BuiltinId`
    - `"sinf" -> BuiltinId::Q32Sin`
    - `"cosf" -> BuiltinId::Q32Cos`
- This will be used by the transform to convert TestCase calls

## Success Criteria

- Test helper functions compile and can be used
- `Q32Sin` and `Q32Cos` are in registry and can be declared
- Mapping table exists and can be queried
- All code compiles without warnings (except unused code that will be used in next phase)

