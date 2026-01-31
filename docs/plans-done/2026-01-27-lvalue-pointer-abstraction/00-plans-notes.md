# Plan Notes: LValue Pointer Abstraction Refactoring

## Context

During implementation of out parameter support, we discovered an architectural inconsistency in how
pointer-based lvalues are handled:

- **Arrays**: Store pointer directly in `LValue::ArrayElement.array_ptr`
- **Out/inout params**: Store pointer in `VarInfo.out_inout_ptr`, accessed via name lookup

This inconsistency forces verbose runtime checks in `read_lvalue()` and `write_lvalue()` functions.

## Current Implementation

### Out/Inout Parameter Handling

When resolving out/inout parameters:

1. Create `LValue::Variable` with `name: Some(...)`
2. At read/write time, check if `name` is Some
3. Look up `VarInfo` by name
4. Check if `out_inout_ptr` is Some
5. Use pointer if found, otherwise use SSA vars

This pattern is repeated in:

- `read_lvalue()` for `Variable` variant
- `read_lvalue()` for `Component` variant
- `write_lvalue()` for `Variable` variant
- `write_lvalue()` for `Component` variant

### Code Locations

**LValue Creation:**

- `lp-glsl/lp-glsl-compiler/src/frontend/codegen/lvalue/resolve/variable.rs`
- `lp-glsl/lp-glsl-compiler/src/frontend/codegen/lvalue/resolve/component/mod.rs`

**LValue Access:**

- `lp-glsl/lp-glsl-compiler/src/frontend/codegen/lvalue/read.rs` (lines 23-77, 79-125)
- `lp-glsl/lp-glsl-compiler/src/frontend/codegen/lvalue/write.rs` (lines 24-96, 98-148)

**Storage:**

- `lp-glsl/lp-glsl-compiler/src/frontend/codegen/context.rs` (`VarInfo.out_inout_ptr`)
- `lp-glsl/lp-glsl-compiler/src/frontend/glsl_compiler.rs` (parameter declaration)

## Questions

### Q1: Should Arrays Also Use Unified Variant?

**Question**: Should `LValue::ArrayElement` also be migrated to `PointerBased`?

**Considerations**:

- Arrays already work well with dedicated variant
- Migration would be more work
- But would provide complete unification

**Answer**: Optional - can be done later if desired. Arrays are working fine as-is.

### Q2: Performance Impact

**Question**: How much performance improvement from eliminating lookups?

**Considerations**:

- Name lookup: HashMap lookup (O(1) average case)
- VarInfo access: Another HashMap lookup
- Pointer check: Simple Option check
- This happens on every read/write of out/inout params

**Answer**: Likely small but measurable improvement. More importantly, it's cleaner code.

### Q3: Backwards Compatibility

**Question**: Can we do this incrementally without breaking existing code?

**Answer**: Yes - add new variant, migrate gradually, remove old code last.

### Q4: Struct Support

**Question**: How will structs fit into this?

**Answer**: Structs will likely be pointer-based (like arrays). Unified variant makes this easier.

### Q5: Name Field Usage

**Question**: Should we keep the `name` field in `Variable` and `Component` variants for other
purposes (debugging, error messages)?

**Considerations**:

- Currently used only as a flag for pointer detection
- Could be useful for error messages and debugging
- But adds complexity if kept

**Answer**: Remove the `name` field after migration. If variable names are needed for error messages
later, we can add them back or access them through other means (like the original AST node or source
location).

### Q6: Out/Inout Array Parameters

**Question**: How should out/inout array parameters be handled?

**Considerations**:

- Arrays as out/inout params currently use `array_ptr` from `VarInfo`
- Should they use `PointerBased` with `Direct` pattern?
- Or keep current array handling separate?

**Answer**: Migrate out/inout arrays to `PointerBased` with `Direct` pattern for full unification.
The `ArrayElement` access pattern in `PointerBased` can still handle array element access (
`arr[i]`), while `Direct` handles the array variable itself (`arr` as an out/inout parameter).

### Q7: VarInfo.out_inout_ptr Cleanup

**Question**: Should we remove `out_inout_ptr` from `VarInfo` after migration, or keep it for
backwards compatibility?

**Considerations**:

- Removing it makes the code cleaner
- Keeping it allows gradual migration
- Could deprecate and remove later

**Answer**: Remove it immediately after migration. We're doing this refactoring in one shot, so
cleaner is better.

## Implementation Phases

### Phase 1: Add PointerBased Variant

- Add variant to `LValue` enum
- Add `PointerAccessPattern` enum
- Update `LValue::ty()` method
- No functional changes yet

### Phase 2: Update Out/Inout Resolution

- Modify `resolve_variable_lvalue()` to create `PointerBased` for out/inout
- Modify component resolution to create `PointerBased` for out/inout components
- Keep old code path temporarily

### Phase 3: Update Read/Write Functions

- Add handling for `PointerBased` variant
- Keep old code path for `Variable`/`Component` with name lookup
- Test that both paths work

### Phase 4: Migrate All Usage

- Update all LValue creation sites
- Ensure all out/inout params use `PointerBased`
- Remove `name` field usage for pointer detection

### Phase 5: Cleanup

- Remove `name` field from `Variable` and `Component` (or keep for other purposes)
- Remove runtime lookups from read/write functions
- Remove `out_inout_ptr` from `VarInfo` (or keep for migration period)

### Phase 6: Testing & Verification

- Run all existing tests
- Add new tests for `PointerBased` variant
- Benchmark performance improvement
- Verify no regressions

## Risks

1. **Breaking Changes**: Risk of breaking existing functionality during migration
    - **Mitigation**: Keep both code paths during migration, test thoroughly

2. **Complexity**: Adding new variant increases enum size
    - **Mitigation**: Variant is well-designed, clear purpose

3. **Migration Effort**: Need to update many sites
    - **Mitigation**: Can be done incrementally, automated refactoring tools can help

## Success Metrics

- [ ] Zero runtime VarInfo lookups in read/write functions
- [ ] All pointer-based lvalues use `PointerBased` variant
- [ ] Code duplication eliminated
- [ ] All tests pass
- [ ] Performance improvement measurable (even if small)
- [ ] Code is cleaner and easier to understand

## Related Work

- Out parameter implementation (2026-01-27-out-parameters)
- Array support (already implemented)
- Future struct support (will benefit from this refactoring)

## Notes

- This is a refactoring, not a feature addition
- Can be done at any time, no dependencies
- Should be done before adding struct support (makes structs easier)
- Consider doing this as a "code quality" improvement

## Code Review Findings

### Current Duplication Pattern

The same pattern appears in 4 places:

1. `read_lvalue()` - `Variable` variant (lines 23-77)
2. `read_lvalue()` - `Component` variant (lines 79-125)
3. `write_lvalue()` - `Variable` variant (lines 24-96)
4. `write_lvalue()` - `Component` variant (lines 98-158)

Each location has:

- Name lookup check: `if let Some(var_name) = name`
- VarInfo lookup: `if let Some(var_info) = ctx.lookup_var_info(var_name)`
- Pointer check: `if let Some(ptr) = var_info.out_inout_ptr`
- Pointer-based load/store logic
- Fallback to SSA vars

### Array Element Pattern (Good Example)

`LValue::ArrayElement` already stores pointer directly:

- `array_ptr: Value` - pointer available at LValue creation
- No runtime lookups needed
- Clean separation of concerns

### Out/Inout Parameter Resolution

Currently in `resolve_variable_lvalue()`:

- Checks `var_info.out_inout_ptr` to determine if out/inout
- Sets `name: Some(...)` if out/inout, `None` otherwise
- This `name` field is used as a flag, but doesn't guarantee pointer exists

### Component Resolution

In `resolve_component_on_variable()`:

- Passes `name` through to `Component` variant
- Same pattern: name lookup → VarInfo → pointer check

### DirectXShaderCompiler Reference

**Architecture Differences:**

DirectXShaderCompiler uses LLVM IR, which fundamentally differs from our approach:

1. **LLVM Arguments as Pointers**: In LLVM IR, function arguments are already Values. For inout
   parameters, they're pointer types (`i32*`, `<4 x float>*`, etc.), so the pointer is inherent in
   the type system.

2. **No Custom LValue Abstraction**: They work directly with LLVM Values/Arguments. No equivalent to
   our `LValue` enum - they use LLVM's type system to distinguish storage models.

3. **Inout Handling**: They process inout parameters twice:
    - First as output (generate StoreOutput calls)
    - Then as input (generate LoadInput calls)
    - Tracked in `m_inoutArgSet` set for validation

4. **Code Generation**: They replace loads/stores with DXIL intrinsics (`LoadInput`/`StoreOutput`)
   during a lowering pass, rather than generating pointer-based code upfront.

**Key Insight**: Their approach works because LLVM's type system already distinguishes pointer
types. Our approach needs to be explicit because Cranelift doesn't have the same semantic
distinction - we need to track whether an LValue uses SSA variables or pointer-based storage.

**Our Design Comparison**: Our unified `PointerBased` variant is conceptually similar to how LLVM
treats pointer-typed arguments - the storage model is explicit in the type. This validates our
approach of making storage model explicit in the LValue type system.
