# Plans Notes: LPFX Refactor Cleanup

## Context

The builtins directory was reorganized:

- Functions moved to `glsl/q32/`, `internal/q32/`, and `lpfx/` directories
- `lp_simplex` and `lp_noise` renamed to use `__lpfx` prefix
- In GLSL usage: functions use `lpfx_` prefix (already correct)
- In implementations: functions named `__lpfx_name_<decimal-format>` (e.g., `__lpfx_snoise1_q32`)

## Problem

The code is doing direct string checks against function names instead of using `LpfxFnId` which
should be the single source of truth. This is brittle and error-prone.

### Issues Found

1. **Generator code (`lp-glsl-builtin-gen-app/src/main.rs`)**:
    - Lines 37, 201-203, 259-262: Direct `starts_with()` checks for `__lpfx_hash_` and
      `__lpfx_snoise`
    - Lines 440, 443, 459, 549, 553, 595, 603, 763: More direct string checks
    - Should use `LpfxFnId` methods instead

2. **Testcase mapping (`math.rs`)**:
    - Generated code uses direct string matching
    - Generator creates mappings based on string checks instead of `LpfxFnId`

3. **Module path determination**:
    - Generator determines module paths (`q32` vs `shared`) based on string checks
    - Should use `LpfxFnId` to determine correct module

4. **Function type determination**:
    - Generator determines function signatures based on string checks
    - Should use `LpfxFnId` methods

## Questions

### Q1: Should we update `LpfxFnId` to provide module path information?

**Context**: The generator needs to know which module (`glsl::q32`, `lpfx::hash`, `lpfx::simplex`) a
function belongs to.

**Suggestion**: Add a method like `module_path()` that returns the module path for the function
implementation.

**Answer**: Add `module_name()` function similar to `symbol_name()` that returns the module name (
e.g., "lpfx::hash", "glsl::q32").

### Q2: Should we update `LpfxFnId` to provide the actual implementation function name?

**Context**: The generator needs to know the actual Rust function name (e.g., `__lpfx_snoise1_q32`)
to generate imports and references.

**Suggestion**: Add a method like `implementation_name()` that returns the actual Rust function
name.

**Answer**: [To be answered]

### Q3: How should we handle the testcase name mapping for hash functions?

**Context**: Currently hash functions map to testcase names like `"1f" | "__lp_1"`. Should this be
derived from `LpfxFnId` or kept as-is?

**Suggestion**: Add a method `testcase_names()` that returns all valid testcase names for a
function.

**Answer**: [To be answered]

### Q4: Should `is_lp_lib_fn` check for `lpfx_` prefix instead of `lp_`?

**Context**: Currently `is_lp_lib_fn` checks for `lp_` prefix, but all functions use `lpfx_` prefix.

**Suggestion**: Update to check for `lpfx_` prefix.

**Answer**: [To be answered]

## Notes

- The reorganization moved files but shouldn't affect functionality - we just need to update the
  codegen
- GLSL names are already correct (`lpfx_hash`, `lpfx_snoise1`, etc.)
- Implementation names follow pattern `__lpfx_name_<format>` which is correct
- Need to ensure all string-based checks are replaced with `LpfxFnId` method calls
