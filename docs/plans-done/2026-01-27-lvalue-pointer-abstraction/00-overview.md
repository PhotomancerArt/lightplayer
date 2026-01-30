# Overview: LValue Pointer Abstraction Refactoring

## Status

**Status**: Planning Complete - Ready for Implementation  
**Priority**: Medium  
**Dependencies**: None (can be done independently)

## Problem Statement

The current LValue abstraction has inconsistent handling of pointer-based storage:

1. **Arrays** (`LValue::ArrayElement`): Pointer stored directly in variant (`array_ptr: Value`)
2. **Out/inout parameters** (`LValue::Variable` / `LValue::Component`): Pointer stored in `VarInfo.out_inout_ptr`, accessed via name lookup
3. **Regular variables**: Use Cranelift `Variable`s (SSA values)

This inconsistency forces verbose runtime checks in `read_lvalue()` and `write_lvalue()`:

```rust
if let Some(var_name) = name {
    if let Some(var_info) = ctx.lookup_var_info(var_name) {
        if let Some(ptr) = var_info.out_inout_ptr {
            // Use pointer...
        }
    }
}
// Otherwise use vars...
```

## Goals

1. **Unify pointer-based storage**: All pointer-based lvalues should use the same abstraction
2. **Eliminate runtime lookups**: Determine storage model at LValue creation time, not read/write time
3. **Improve type safety**: Make storage model explicit in the type system
4. **Reduce code duplication**: Remove repeated pointer-checking logic

## Current State Analysis

### Pointer-Based LValues

| LValue Type             | Storage Location                   | Access Pattern |
| ----------------------- | ---------------------------------- | -------------- |
| `ArrayElement`          | `array_ptr: Value` (in variant)    | Direct access  |
| `Variable` (out/inout)  | `VarInfo.out_inout_ptr` (via name) | Runtime lookup |
| `Component` (out/inout) | `VarInfo.out_inout_ptr` (via name) | Runtime lookup |
| `Variable` (regular)    | `vars: Vec<Variable>` (SSA)        | Direct access  |
| `Component` (regular)   | `base_vars: Vec<Variable>` (SSA)   | Direct access  |

### Issues

1. **Dual-purpose variants**: `Variable` and `Component` serve both SSA and pointer-based storage
2. **Implicit storage model**: `name: Option<String>` is used as a flag, but doesn't guarantee pointer-based storage
3. **Runtime overhead**: Name lookup and VarInfo access on every read/write
4. **Code duplication**: Same checking logic repeated in multiple places

## Proposed Solutions

### Option 1: Unified Pointer Variant (Recommended)

Create a dedicated variant for pointer-based storage:

```rust
pub enum LValue {
    Variable { vars: Vec<Variable>, ty: GlslType },
    Component { base_vars: Vec<Variable>, base_ty: GlslType, indices: Vec<usize>, result_ty: GlslType },
    PointerBased {
        ptr: Value,
        base_ty: GlslType,
        access_pattern: PointerAccessPattern,
    },
    // ... other variants
}

enum PointerAccessPattern {
    Direct,  // Full variable/vector/matrix
    Component { indices: Vec<usize> },
    ArrayElement { index: Option<usize>, index_val: Option<Value>, element_size_bytes: usize },
}
```

**Pros**:

- Clear separation of storage models
- No runtime lookups needed
- Type-safe: can't accidentally mix storage models

**Cons**:

- Requires refactoring existing code
- Need to update all LValue creation sites

### Option 2: Store Pointer in Variant

Add optional pointer field to existing variants:

```rust
pub enum LValue {
    Variable {
        vars: Vec<Variable>,
        ty: GlslType,
        ptr: Option<Value>,  // If Some, use pointer; otherwise use vars
    },
    Component {
        base_vars: Vec<Variable>,
        base_ty: GlslType,
        indices: Vec<usize>,
        result_ty: GlslType,
        ptr: Option<Value>,  // If Some, use pointer; otherwise use vars
    },
    // ...
}
```

**Pros**:

- Minimal changes to existing code
- Pointer available at LValue creation time
- No runtime lookups

**Cons**:

- Still dual-purpose variants
- `Option<Value>` adds some complexity

### Option 3: Helper Trait/Abstraction

Create a trait that abstracts storage access:

```rust
trait LValueStorage {
    fn get_storage_location(&self, ctx: &CodegenContext) -> StorageLocation;
}

enum StorageLocation {
    SSA(Vec<Variable>),
    Pointer(Value),
}
```

**Pros**:

- Clean abstraction
- Can be added incrementally

**Cons**:

- Still requires runtime checks (moved to trait)
- Doesn't solve the fundamental inconsistency

## Recommendation

**Option 1 (Unified Pointer Variant)** is recommended because:

- It provides the clearest separation of concerns
- Eliminates all runtime lookups
- Makes the storage model explicit in the type system
- Aligns with how arrays are already handled

## Implementation Phases

1. **Add PointerBased Variant** (`01-add-pointerbased-variant.md`): Add `PointerBased` variant and `PointerAccessPattern` enum to `LValue`, update `ty()` method
2. **Update Out/Inout Resolution** (`02-update-out-inout-resolution.md`): Modify variable and component resolution to create `PointerBased` for out/inout parameters
3. **Update Read/Write Functions** (`03-update-read-write-functions.md`): Add handling for `PointerBased` variant in read/write functions, keep old code path temporarily
4. **Migrate All Usage** (`04-migrate-all-usage.md`): Update all LValue creation sites, ensure all out/inout params use `PointerBased`, remove `name` field usage
5. **Remove Old Code** (`05-remove-old-code.md`): Remove `name` fields, runtime lookups, and `out_inout_ptr` from `VarInfo`
6. **Testing & Cleanup** (`06-testing-cleanup.md`): Run all tests, add new tests for `PointerBased`, verify no regressions, cleanup

## Success Criteria

- [ ] No runtime VarInfo lookups in read/write functions
- [ ] Storage model explicit in LValue type
- [ ] All pointer-based lvalues use same abstraction
- [ ] Code duplication eliminated
- [ ] All existing tests pass
- [ ] Performance improvement (eliminate lookups)

## Files Affected

- `lp-glsl/crates/lp-glsl-compiler/src/frontend/codegen/lvalue/types.rs`
- `lp-glsl/crates/lp-glsl-compiler/src/frontend/codegen/lvalue/read.rs`
- `lp-glsl/crates/lp-glsl-compiler/src/frontend/codegen/lvalue/write.rs`
- `lp-glsl/crates/lp-glsl-compiler/src/frontend/codegen/lvalue/resolve/variable.rs`
- `lp-glsl/crates/lp-glsl-compiler/src/frontend/codegen/lvalue/resolve/component/mod.rs`
- `lp-glsl/crates/lp-glsl-compiler/src/frontend/glsl_compiler.rs`

## Notes

- This refactoring can be done incrementally
- Consider keeping both old and new code paths during migration
- May want to add benchmarks to measure performance improvement
- Should coordinate with any future struct support (structs will likely also be pointer-based)
