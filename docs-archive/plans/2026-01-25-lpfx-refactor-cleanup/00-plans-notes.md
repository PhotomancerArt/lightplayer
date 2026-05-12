# Plans Notes: LPFX Refactor Cleanup

## Context

The builtins directory was reorganized:

- Functions moved to `glsl/q32/`, `internal/q32/`, and `lpfn/` directories
- `lp_simplex` and `lp_noise` renamed to use `__lpfn` prefix
- In GLSL usage: functions use `lpfn_` prefix (already correct)
- In implementations: functions named `__lpfn_name_<decimal-format>` (e.g., `__lpfn_snoise1_q32`)

## Problem

The code is doing direct string checks against function names instead of using `LpfnFnId` which
should be the single source of truth. This is brittle and error-prone.

### Issues Found

1. **Generator code (`lps-builtin-gen-app/src/main.rs`)**:
    - Lines 37, 201-203, 259-262: Direct `starts_with()` checks for `__lpfn_hash_` and
      `__lpfn_snoise`
    - Lines 440, 443, 459, 549, 553, 595, 603, 763: More direct string checks
    - Should use `LpfnFnId` methods instead

2. **Testcase mapping (`math.rs`)**:
    - Generated code uses direct string matching
    - Generator creates mappings based on string checks instead of `LpfnFnId`

3. **Module path determination**:
    - Generator determines module paths (`q32` vs `shared`) based on string checks
    - Should use `LpfnFnId` to determine correct module

4. **Function type determination**:
    - Generator determines function signatures based on string checks
    - Should use `LpfnFnId` methods

## Questions

### Q1: Should we update `LpfnFnId` to provide module path information?

**Context**: The generator needs to know which module (`glsl::q32`, `lpfn::hash`, `lpfn::simplex`) a
function belongs to.

**Suggestion**: Add a method like `module_path()` that returns the module path for the function
implementation.

**Answer**: Add `module_name()` function similar to `symbol_name()` that returns the module name (
e.g., "lpfn::hash", "glsl::q32").

### Q2: Should we update `LpfnFnId` to provide the actual implementation function name?

**Context**: The generator needs to know the actual Rust function name (e.g., `__lpfn_snoise1_q32`)
to generate imports and references.

**Suggestion**: Add a method like `implementation_name()` that returns the actual Rust function
name.

**Answer**: [To be answered]

### Q3: How should we handle the testcase name mapping for hash functions?

**Context**: Currently hash functions map to testcase names like `"1f" | "__lp_1"`. Should this be
derived from `LpfnFnId` or kept as-is?

**Suggestion**: Add a method `testcase_names()` that returns all valid testcase names for a
function.

**Answer**: [To be answered]

### Q4: Should `is_lp_lib_fn` check for `lpfn_` prefix instead of `lp_`?

**Context**: Currently `is_lp_lib_fn` checks for `lp_` prefix, but all functions use `lpfn_` prefix.

**Suggestion**: Update to check for `lpfn_` prefix.

**Answer**: [To be answered]

## Notes

- The reorganization moved files but shouldn't affect functionality - we just need to update the
  codegen
- GLSL names are already correct (`lpfn_hash`, `lpfn_snoise1`, etc.)
- Implementation names follow pattern `__lpfn_name_<format>` which is correct
- Need to ensure all string-based checks are replaced with `LpfnFnId` method calls
