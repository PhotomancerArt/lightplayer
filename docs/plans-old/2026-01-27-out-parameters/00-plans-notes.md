# Plan Notes: Out Parameter Support

## Context

Out parameters (`out` and `inout` qualifiers) are currently parsed and stored in function signatures but not implemented in codegen. This is needed for:

- User-defined GLSL functions with out/inout parameters
- Native/LPFX functions like `psrdnoise` that use out parameters

Current state:

- ✅ Parsing: `out`/`inout` qualifiers are recognized and stored
- ❌ Function signatures: Parameters passed by value (should be pointers for out/inout)
- ❌ Function calls: No pointer passing or copy-back logic
- ❌ Function definitions: Parameters loaded as values (should dereference pointers)
- ❌ Native functions: No support for out parameters in LPFX signatures

## Questions

### Q1: Parameter Passing Strategy

**Question**: How should out/inout parameters be passed in function signatures?

**Context**: Currently all parameters are passed by value (expanded to components for vectors/matrices). For out/inout parameters, we need to pass pointers so the callee can write back.

**Suggested Answer**:

- Out/inout parameters should be passed as pointers (single pointer per parameter, regardless of type)
- For vectors/matrices, pass a pointer to the first element (like arrays)
- Use `pointer_type` from ISA for all out/inout parameters
- In parameters continue to be passed by value (expanded to components)

**Alternative Considered**: Pass all parameters as pointers - rejected because it's less efficient and doesn't match GLSL semantics where `in` parameters are copy-in only.

**Answer**: ✅ Confirmed - Out/inout parameters passed as pointers, in parameters by value.

---

### Q2: Function Call Implementation

**Question**: How should function calls handle out/inout arguments?

**Context**: When calling a function with out/inout parameters:

1. Need to pass addresses of lvalues (not values)
2. Need to copy values back after the call (for out/inout)
3. Need to validate that arguments are lvalues

**Suggested Answer**:

- For out/inout arguments: Get address of the lvalue (variable/array element/etc.)
- Pass pointer as argument
- After function call completes: Load values from the pointer and store back to the original lvalue
- Validate lvalue requirement in semantic checking phase (before codegen)

**Answer**: ✅ Confirmed - Copy-back happens immediately after call. For multiple out parameters, copy back in order (left to right, matching parameter order).

---

### Q3: Function Definition Implementation

**Question**: How should function definitions handle out/inout parameters?

**Context**: In function definitions, parameters are currently loaded as values from block parameters. For out/inout, they should be pointers that get dereferenced when accessed.

**Suggested Answer**:

- Out/inout parameters arrive as pointers in block parameters
- When declaring parameter as variable: Store the pointer, don't load the value
- When reading parameter: Load from pointer (for inout, also for initial value of out)
- When writing parameter: Store to pointer
- This requires changes to how parameters are declared and how lvalue resolution works for parameters

**Answer**: ✅ Confirmed - Out parameters start uninitialized (undefined behavior to read before write). Component access (e.g., `result.x`) works by computing offset from pointer and loading/storing at that offset.

---

### Q4: Native/LPFX Function Support

**Question**: How should native/LPFX functions support out parameters?

**Context**: Need to implement `psrdnoise` which likely has out parameters. Native functions are registered via `lpfx_sig.rs` and `build_call_signature()`.

**Suggested Answer**:

- Extend `build_call_signature()` to check parameter qualifiers
- For out/inout parameters: Add pointer type to signature instead of value types
- Update signature building to handle mixed in/out/inout parameters
- Native functions receive pointers and write to them directly

**Answer**: ✅ Confirmed - Support out parameters only for LPFX functions (not all native functions). For vector out parameters in native functions, use single pointer to first element (like arrays).

---

### Q5: Lvalue Validation

**Question**: When should lvalue validation happen for out/inout arguments?

**Context**: GLSL requires that out/inout arguments must be lvalues (variables, array elements, etc.), not expressions.

**Suggested Answer**:

- Validate in semantic checking phase (before codegen)
- Check that argument expression is an lvalue when parameter qualifier is out/inout
- Emit error if non-lvalue is passed to out/inout parameter

**Answer**: ✅ Confirmed - Compile-time error. Valid lvalues are: variables, array elements, vector components (swizzles), struct fields, matrix elements/columns. Can reuse existing `resolve_lvalue()` function - if it succeeds, it's a valid lvalue.

---

### Q6: Test Coverage

**Question**: What additional tests are needed beyond existing filetests?

**Context**: We have `param-out.glsl`, `param-inout.glsl`, `param-mixed.glsl`, and `edge-lvalue-out.glsl` tests, but they're currently failing.

**Suggested Answer**:

- Review existing tests for completeness
- Add tests for:
  - Out parameters with struct types (if supported)
  - Out parameters in recursive functions
  - Out parameters with const correctness
  - Error cases: passing non-lvalue to out parameter
  - Performance: multiple out parameters
  - Edge cases: out parameter aliasing (passing same variable twice)

**Answer**: ✅ Confirmed - Keep tests generic (no psrdnoise-specific tests needed). No separate tests needed for native function out parameters (they'll be covered by LPFX function tests). Skip struct tests (no struct support yet). Add tests for array support in out/inout parameters.

---

## Notes

- The comment "Phase 8" in code suggests this was planned but deferred
- `executable.rs` trait mentions out/inout as future work
- Similar pattern exists for StructReturn (pointer for return values) - can reuse some concepts
