# Phase 2: Constant Array Indexing (AccessIndex)

## Scope

Implement `AccessIndex` lowering for arrays to enable constant-index element access: `arr[0]`, `arr[2]`, etc.

Target: `test_read()`, `test_write()`, `test_multiple_writes()`, `test_multiple_reads()`, `phase1()` from `1-foundation.glsl` should pass.

## Implementation Details

### 1. Detect Array AccessIndex in expr_type_inner

In `lp-glsl/lp-glsl-naga/src/naga_util.rs`, `expr_type_inner()` already handles `AccessIndex` on arrays at lines 232-234:

```rust
TypeInner::Array { base: elt, .. } => Ok(module.types[*elt].inner.clone()),
```

This should already work - verify it's returning the correct element type.

### 2. Add Array AccessIndex Lowering

In `lp-glsl/lp-glsl-naga/src/lower_expr.rs`, extend the `AccessIndex` match arm:

```rust
Expression::AccessIndex { base, index } => {
    let base_inner = expr_type_inner(ctx.module, ctx.func, *base)?;
    match base_inner {
        // ... existing vector/matrix handling ...
        
        TypeInner::Pointer { base: ty_h, space } => {
            let inner = &ctx.module.types[*ty_h].inner;
            match inner {
                // ... existing vector/matrix handling ...
                
                TypeInner::Array { base: elem, .. } => {
                    // Array access with constant index
                    let Expression::LocalVariable(lv) = &ctx.func.expressions[*base] else {
                        return Err(LowerError::UnsupportedExpression(
                            "AccessIndex: array pointer base must be LocalVariable".into()
                        ));
                    };
                    
                    let array_info = ctx.resolve_array(*lv)?;
                    let elem_inner = &ctx.module.types[*elem].inner;
                    let elem_ir_types = naga_type_to_ir_types(elem_inner)?;
                    
                    // Compute byte offset: index * element_size
                    let byte_offset = (*index as u32) * array_info.element_size;
                    
                    // Get slot base address
                    let base_addr = ctx.fb.alloc_vreg(IrType::I32);
                    ctx.fb.push(Op::SlotAddr {
                        dst: base_addr,
                        slot: array_info.slot,
                    });
                    
                    // Load each component of the element
                    let mut result = VRegVec::new();
                    for (i, ty) in elem_ir_types.iter().enumerate() {
                        let dst = ctx.fb.alloc_vreg(*ty);
                        let offset = byte_offset + (i as u32 * 4); // 4 bytes per component
                        ctx.fb.push(Op::Load {
                            dst,
                            base: base_addr,
                            offset,
                        });
                        result.push(dst);
                    }
                    
                    Ok(result)
                }
                
                _ => Err(LowerError::UnsupportedExpression(
                    format!("AccessIndex on pointer to {inner:?}")
                )),
            }
        }
        
        other => Err(LowerError::UnsupportedExpression(
            format!("AccessIndex on {other:?}")
        )),
    }
}
```

### 3. Add Store Support for Array Elements

In `lp-glsl/lp-glsl-naga/src/lower_stmt.rs`, handle `Store` where the pointer is an array element access:

```rust
Statement::Store { pointer, value } => {
    match &ctx.func.expressions[*pointer] {
        Expression::LocalVariable(lv) => {
            // ... existing non-array store handling ...
        }
        
        Expression::AccessIndex { base, index } => {
            let base_expr = &ctx.func.expressions[*base];
            if let Expression::LocalVariable(lv) = base_expr {
                // Check if base is an array
                if let Some(array_info) = ctx.array_map.get(lv) {
                    // Array element store
                    let byte_offset = (*index as u32) * array_info.element_size;
                    
                    // Get value to store
                    let srcs = ctx.ensure_expr_vec(*value)?;
                    
                    // Get slot base address
                    let base_addr = ctx.fb.alloc_vreg(IrType::I32);
                    ctx.fb.push(Op::SlotAddr {
                        dst: base_addr,
                        slot: array_info.slot,
                    });
                    
                    // Store each component
                    for (i, &src) in srcs.iter().enumerate() {
                        let offset = byte_offset + (i as u32 * 4);
                        ctx.fb.push(Op::Store {
                            base: base_addr,
                            offset,
                            value: src,
                        });
                    }
                    
                    return Ok(());
                }
            }
            
            // Fall through to existing AccessIndex store handling
            // ... existing code ...
        }
        
        // ... other cases ...
    }
}
```

## Tests to Verify

From `array/phase/1-foundation.glsl`:
```glsl
int test_write() {
    int arr[5];
    arr[0] = 10;
    return 0;
}
// run: test_write() == 0

int test_read() {
    int arr[5];
    arr[0] = 10;
    int x = arr[0];
    return x;
}
// run: test_read() == 10

int test_multiple_writes() {
    int arr[5];
    arr[0] = 10;
    arr[1] = 20;
    arr[2] = 30;
    return 0;
}
// run: test_multiple_writes() == 0

int test_multiple_reads() {
    int arr[5];
    arr[0] = 10;
    arr[2] = 30;
    arr[4] = 50;
    int x = arr[0];
    int y = arr[2];
    int z = arr[4];
    return x + y + z;
}
// run: test_multiple_reads() == 90

int phase1() {
    int arr[5];
    arr[0] = 10;
    arr[1] = 20;
    arr[2] = 30;
    arr[3] = 40;
    arr[4] = 50;
    int x = arr[0];
    int y = arr[2];
    int z = arr[4];
    return x + y + z;
}
// run: phase1() == 90
```

## Validation

```bash
scripts/glsl-filetests.sh array/phase/1-foundation.glsl
```

Expected: All 6 tests in 1-foundation.glsl pass.

## Code Quality Notes

- Reuse the element_size calculation from ArrayInfo
- Offset computation: `index * element_size + component_offset`
- For scalars, component_offset is always 0
- Place array-specific store logic in a helper function at bottom of file
- Consider extracting "store to array element" as a helper for reuse
