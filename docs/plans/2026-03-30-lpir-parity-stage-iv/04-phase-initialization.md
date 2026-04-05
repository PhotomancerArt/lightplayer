# Phase 4: Array Initialization

## Scope

Implement array initializer list support: `int arr[3] = {10, 20, 30}`, including full
initialization, partial initialization with zero-fill, and unsized arrays with inferred size.

Target: All 5 tests from `3-initialization.glsl` should pass.

## Implementation Details

### 1. Detect Initializer in Local Variable Declaration

In `lp-shader/lps-naga/src/lower_ctx.rs`, the local variable init handling already exists for
non-arrays:

```rust
for (lv_handle, var) in func.local_variables.iter() {
    if ctx.param_aliases.contains_key(&lv_handle) {
        continue;
    }
    let Some(init_h) = var.init else {
        continue;
    };
    
    // Check if this is an array
    if let Some(array_info) = ctx.array_map.get(&lv_handle) {
        lower_array_initializer(ctx, *lv_handle, array_info, init_h)?;
    } else {
        // Existing non-array initialization
        let dsts = ctx.local_map.get(&lv_handle).cloned().ok_or_else(|| ...)?;
        let srcs = lower_expr::lower_expr_vec(ctx, init_h)?;
        // ... copy to dsts ...
    }
}
```

### 2. Add Array Initializer Lowering

Add to `lp-shader/lps-naga/src/lower_array.rs`:

```rust
/// Lower array initializer: {a, b, c} or unsized array inference
pub(crate) fn lower_array_initializer(
    ctx: &mut LowerCtx<'_>,
    lv: Handle<LocalVariable>,
    array_info: &ArrayInfo,
    init_expr: Handle<naga::Expression>,
) -> Result<(), LowerError> {
    match &ctx.func.expressions[init_expr] {
        Expression::Compose { components, .. } => {
            // Full or partial initialization: int arr[5] = {1, 2, 3}
            lower_array_compose(ctx, array_info, components)?;
        }
        
        _ => {
            return Err(LowerError::UnsupportedExpression(
                "array initializer must be a compose expression".into()
            ));
        }
    }
    
    Ok(())
}

/// Lower compose-style array initializer with optional zero-fill
fn lower_array_compose(
    ctx: &mut LowerCtx<'_>,
    array_info: &ArrayInfo,
    components: &[Handle<naga::Expression>],
) -> Result<(), LowerError> {
    let num_init = components.len() as u32;
    let num_total = array_info.element_count;
    
    // Get slot base address
    let base_addr = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(Op::SlotAddr {
        dst: base_addr,
        slot: array_info.slot,
    });
    
    // Store each initializer element
    for (i, &comp_expr) in components.iter().enumerate() {
        let byte_offset = (i as u32) * array_info.element_size;
        store_element_at_offset(ctx, array_info, base_addr, byte_offset, comp_expr)?;
    }
    
    // Zero-fill remaining elements if partial initialization
    if num_init < num_total {
        // Create zero value(s) for element type
        let elem_inner = &ctx.module.types[array_info.element_ty].inner;
        let elem_ir_types = naga_type_to_ir_types(elem_inner)?;
        
        for i in num_init..num_total {
            let byte_offset = i * array_info.element_size;
            
            // Store zero for each component of the element
            for (j, ty) in elem_ir_types.iter().enumerate() {
                let zero = ctx.fb.alloc_vreg(*ty);
                if *ty == IrType::F32 {
                    ctx.fb.push(Op::FconstF32 { dst: zero, value: 0.0 });
                } else {
                    ctx.fb.push(Op::IconstI32 { dst: zero, value: 0 });
                }
                
                let comp_offset = byte_offset + (j as u32 * 4);
                ctx.fb.push(Op::Store {
                    base: base_addr,
                    offset: comp_offset,
                    value: zero,
                });
            }
        }
    }
    
    Ok(())
}

/// Store a single element at a byte offset in the array
fn store_element_at_offset(
    ctx: &mut LowerCtx<'_>,
    array_info: &ArrayInfo,
    base_addr: VReg,
    byte_offset: u32,
    expr: Handle<naga::Expression>,
) -> Result<(), LowerError> {
    let srcs = ctx.ensure_expr_vec(expr)?;
    
    for (i, &src) in srcs.iter().enumerate() {
        let offset = byte_offset + (i as u32 * 4);
        ctx.fb.push(Op::Store {
            base: base_addr,
            offset,
            value: src,
        });
    }
    
    Ok(())
}
```

### 3. Handle Unsized Arrays

For `int arr[] = {1, 2, 3}`, Naga infers the size from the initializer. The array type in Naga will
have the correct size after parsing. Our existing slot allocation code should work as-is because it
reads the size from the type.

However, we need to verify this works correctly. In `lower_ctx.rs`:

```rust
// When processing local variables, check for unsized array that now has size
let ty_inner = &module.types[var.ty].inner;
if let TypeInner::Array { base, size } = ty_inner {
    let size_val = size.nice_unwrap();
    // Naga should have resolved unsized arrays by now
    // Just verify size > 0
    if size_val == 0 {
        return Err(LowerError::UnsupportedType(
            "unsized array without initializer".into()
        ));
    }
    // ... proceed with allocation ...
}
```

## Tests to Verify

From `array/phase/3-initialization.glsl`:

```glsl
int test_full_initialization() {
    int arr[3] = {10, 20, 30};
    return arr[0] + arr[1] + arr[2];
}
// run: test_full_initialization() == 60

int test_partial_initialization() {
    int arr[5] = {1, 2, 3};
    // arr[3] and arr[4] should be 0
    return arr[0] + arr[1] + arr[2] + arr[3] + arr[4];
}
// run: test_partial_initialization() == 6

int test_unsized_array() {
    int arr[] = {100, 200, 300};
    return arr[0] + arr[1] + arr[2];
}
// run: test_unsized_array() == 600

int test_single_element_initialization() {
    int arr[3] = {42};
    return arr[0] + arr[1] + arr[2];
}
// run: test_single_element_initialization() == 42

int phase3() {
    int arr1[3] = {10, 20, 30};
    int x = arr1[0] + arr1[1] + arr1[2];
    
    int arr2[5] = {1, 2, 3};
    int y = arr2[0] + arr2[4];
    
    int arr3[] = {100, 200, 300};
    int z = arr3[0] + arr3[2];
    
    return x + y + z;
}
// run: phase3() == 461
```

## Validation

```bash
scripts/glsl-filetests.sh array/phase/3-initialization.glsl
```

Expected: All 5 tests pass.

## Code Quality Notes

- Zero-fill uses individual stores per element - simple but not optimal
- For large arrays, consider using `Memcpy` from a zero slot in future
- Unsized arrays are resolved by Naga before we see them
- Place helper functions at bottom of `lower_array.rs`
- No changes needed for array_map structure
