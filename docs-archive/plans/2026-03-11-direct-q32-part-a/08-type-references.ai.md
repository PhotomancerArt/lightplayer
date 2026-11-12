# Phase 8: Update Type References

Several codegen files reference `types::F32` directly for load/store
operations, variable declarations, and signatures. These need to use
`ctx.float_type()` (which delegates to `numeric.scalar_type()`) instead.

## `expr/constructor.rs`

Matrix constructor declares temporary variables with F32 type:

```rust
// Before:
let var = ctx.builder.declare_var(cranelift_codegen::ir::types::F32);

// After:
let var = ctx.builder.declare_var(ctx.float_type());
```

1 site.

## `lvalue/write.rs`

Matrix element stores use F32 as the store type:

```rust
// Before:
ctx.builder.ins().store(MemFlags::new(), val, ptr, offset as i32);
// where the size is determined by types::F32

// After: same, but if the type is used to compute offsets or
// select the store instruction, use ctx.float_type()
```

3 sites. Check whether the F32 reference is used for offset calculation
(element size = 4 bytes for both F32 and I32, so this may be a no-op
in practice).

## `lvalue/read.rs`

Matrix element loads:

```rust
// Before:
ctx.builder.ins().load(cranelift_codegen::ir::types::F32, MemFlags::new(), ptr, offset);

// After:
ctx.builder.ins().load(ctx.float_type(), MemFlags::new(), ptr, offset);
```

3 sites.

## `lvalue/resolve/indexing/helpers.rs`

Matrix element type reference:

```rust
// Before:
cranelift_codegen::ir::types::F32

// After:
// This needs access to the float type. If the function doesn't have
// ctx available, pass the scalar type as a parameter.
```

1 site. May need to thread the scalar type through as a parameter.

## `lvalue/resolve/indexing/nested.rs`

1 site, same pattern as helpers.rs.

## `expr/component.rs`

Matrix array element type:

1 site.

## `expr/function.rs`

Matrix parameter load type:

1 site.

## `frontend/codegen/signature.rs`

The signature builder uses `Type::Float.to_cranelift_type()` which returns
`types::F32`. For Plan A (FloatStrategy only), this is unchanged. For
Plan B+, the signature builder will need to accept the scalar type.

Two options:
- Add a `scalar_type: Type` parameter to `SignatureBuilder::build_with_triple`
- Or use `strategy.map_signature()` after building with float types

The second option is cleaner — build float signatures as today, then let
the strategy map them. This defers the signature change to Plan D.

For Plan A: no change needed in signature.rs.

## `frontend/semantic/types.rs`

`to_cranelift_type()` returns `types::F32` for `Type::Float`. This is a
semantic-level mapping, not a codegen-level one. For Plan A, leave it
unchanged. For Plan D, the strategy's `map_signature` handles the
remapping.

## Total: ~11 sites

Some of these may need the scalar type threaded through function parameters
(for lvalue helpers that don't have direct access to CodegenContext). Evaluate
case-by-case — if a function already takes `ctx: &mut CodegenContext`,
use `ctx.float_type()`. If it takes loose parameters, add a `scalar_type: Type`
parameter.

## Approach for lvalue files

The lvalue read/write files use `types::F32` for load/store instructions
on matrix elements. The pattern is:

```rust
builder.ins().load(types::F32, ...)
builder.ins().store(MemFlags::new(), val, ptr, offset)
```

For loads, the type parameter tells cranelift what type to load. For Q32,
this would be `types::I32`. So these MUST use `ctx.float_type()`.

For stores, the type is inferred from the value being stored. No type
parameter needed (cranelift infers it). So stores may not need changes —
verify each site.
