# Phase 3: Dynamic Array Indexing with Bounds Clamping

## Scope

Implement `Expression::Access` lowering for arrays with runtime index values, including bounds
clamping for safety.

Target: All 10 tests from `2-bounds-checking.glsl` should pass.

## Implementation Details

### 1. Add Bounds Clamping Helper

Create `lp-shader/lps-frontend/src/lower_array.rs`:

```rust
//! Array-specific lowering helpers

use lpir::{IrType, Op, VReg};
use crate::lower_ctx::LowerCtx;
use crate::lower_error::LowerError;

/// Clamp index to valid array bounds [0, length-1] using select chains
/// Returns a VReg containing the clamped index
pub(crate) fn clamp_index(
    ctx: &mut LowerCtx<'_>,
    index_v: VReg,
    length: u32,
) -> Result<VReg, LowerError> {
    // Create constants needed for clamping
    let zero = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(Op::IconstI32 { dst: zero, value: 0 });

    let max_idx = (length - 1) as i32;
    let max = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(Op::IconstI32 { dst: max, value: max_idx });

    // Check if index < 0 (for signed comparison, we check if negative)
    // Note: Naga indices are typically signed (i32)
    // For simplicity, we treat as unsigned and compare

    // Step 1: Check if index >= length (unsigned comparison)
    let ge_len = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(Op::IgeU {
        dst: ge_len,
        lhs: index_v,
        rhs: max,  // Actually we need length, not max
    });

    // Wait - we need length (5), not max index (4) for comparison
    // Let me redo this

    let len_v = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(Op::IconstI32 {
        dst: len_v,
        value: length as i32
    });

    // Check if index >= length (out of bounds high)
    let ge_len = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(Op::IgeU {
        dst: ge_len,
        lhs: index_v,
        rhs: len_v,
    });

    // If ge_len, use max, else use index
    let clamped_high = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(Op::Select {
        dst: clamped_high,
        cond: ge_len,
        if_true: max,
        if_false: index_v,
    });

    // For signed indices, we also need to check if index < 0
    // In unsigned, negative becomes large positive, so it's caught by ge_len
    // But for correctness with signed semantics:

    // Check if index < 0 (signed comparison)
    let lt_zero = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(Op::IltS {
        dst: lt_zero,
        lhs: index_v,
        rhs: zero,
    });

    // If lt_zero, use 0, else use clamped_high
    let clamped = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(Op::Select {
        dst: clamped,
        cond: lt_zero,
        if_true: zero,
        if_false: clamped_high,
    });

    Ok(clamped)
}
```

### 2. Add Dynamic Array Access Lowering

In `lp-shader/lps-frontend/src/lower_expr.rs`, add to the `Expression::Access` handling:

```rust
Expression::Access { base, index } => {
    // First, check if base is a nested Access (matrix element, etc.)
    // ... existing nested access handling ...

    // Check if base is an array LocalVariable
    match &ctx.func.expressions[*base] {
        Expression::LocalVariable(lv) => {
            // Check if this is an array
            if let Some(array_info) = ctx.array_map.get(lv).cloned() {
                return lower_array_access(ctx, *index, &array_info);
            }

            // Fall through to existing vector/matrix handling
            // ... existing code ...
        }

        // ... other base cases ...
    }
}

/// Lower dynamic array access with bounds clamping
fn lower_array_access(
    ctx: &mut LowerCtx<'_>,
    index_expr: Handle<naga::Expression>,
    array_info: &ArrayInfo,
) -> Result<VRegVec, LowerError> {
    use crate::lower_array::clamp_index;

    // Get the runtime index value
    let index_v = ctx.ensure_expr(index_expr)?;

    // Clamp to valid bounds
    let clamped_idx = clamp_index(ctx, index_v, array_info.element_count)?;

    // Compute byte offset: clamped_idx * element_size
    let elem_size_imm = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(Op::IconstI32 {
        dst: elem_size_imm,
        value: array_info.element_size as i32
    });

    let byte_offset = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(Op::Imul {
        dst: byte_offset,
        lhs: clamped_idx,
        rhs: elem_size_imm,
    });

    // Get slot base address
    let base_addr = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(Op::SlotAddr {
        dst: base_addr,
        slot: array_info.slot,
    });

    // Compute final address: base + byte_offset
    let final_addr = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(Op::Iadd {
        dst: final_addr,
        lhs: base_addr,
        rhs: byte_offset,
    });

    // Load element
    let elem_inner = &ctx.module.types[array_info.element_ty].inner;
    let elem_ir_types = naga_type_to_ir_types(elem_inner)?;

    let mut result = VRegVec::new();
    for (i, ty) in elem_ir_types.iter().enumerate() {
        let dst = ctx.fb.alloc_vreg(*ty);
        let offset = i as u32 * 4; // Component offset within element
        ctx.fb.push(Op::Load {
            dst,
            base: final_addr,
            offset,
        });
        result.push(dst);
    }

    Ok(result)
}
```

### 3. Add Dynamic Array Store Support

In `lp-shader/lps-frontend/src/lower_stmt.rs`, handle `Access` in Store:

```rust
Statement::Store { pointer, value } => {
    match &ctx.func.expressions[*pointer] {
        // ... existing cases ...

        Expression::Access { base, index } => {
            if let Expression::LocalVariable(lv) = &ctx.func.expressions[*base] {
                if let Some(array_info) = ctx.array_map.get(lv).cloned() {
                    return store_array_dynamic(ctx, *index, &array_info, *value);
                }
            }
            // Fall through to existing handling
        }

        // ... other cases ...
    }
}

/// Store to array element with dynamic index and bounds clamping
fn store_array_dynamic(
    ctx: &mut LowerCtx<'_>,
    index_expr: Handle<naga::Expression>,
    array_info: &ArrayInfo,
    value_expr: Handle<naga::Expression>,
) -> Result<(), LowerError> {
    use crate::lower_array::clamp_index;

    // Get index and clamp
    let index_v = ctx.ensure_expr(index_expr)?;
    let clamped_idx = clamp_index(ctx, index_v, array_info.element_count)?;

    // Compute byte offset
    let elem_size_imm = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(Op::IconstI32 {
        dst: elem_size_imm,
        value: array_info.element_size as i32
    });

    let byte_offset = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(Op::Imul {
        dst: byte_offset,
        lhs: clamped_idx,
        rhs: elem_size_imm,
    });

    // Get base address
    let base_addr = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(Op::SlotAddr {
        dst: base_addr,
        slot: array_info.slot,
    });

    let final_addr = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(Op::Iadd {
        dst: final_addr,
        lhs: base_addr,
        rhs: byte_offset,
    });

    // Get values to store
    let srcs = ctx.ensure_expr_vec(value_expr)?;

    // Store each component
    for (i, &src) in srcs.iter().enumerate() {
        let offset = i as u32 * 4;
        ctx.fb.push(Op::Store {
            base: final_addr,
            offset,
            value: src,
        });
    }

    Ok(())
}
```

## Tests to Verify

From `array/phase/2-bounds-checking.glsl`:

```glsl
// Valid bounds tests
int test_bounds_index_zero() { int arr[3]; arr[0] = 42; return arr[0]; }
// run: test_bounds_index_zero() == 42

int test_bounds_index_middle() { int arr[3]; arr[1] = 100; return arr[1]; }
// run: test_bounds_index_middle() == 100

int test_bounds_index_last() { int arr[3]; arr[2] = 200; return arr[2]; }
// run: test_bounds_index_last() == 200

// Clamping tests (OOB clamps to valid range)
int test_bounds_negative_index_read() {
    int arr[3]; arr[0]=1; arr[1]=2; arr[2]=3;
    int i=-1; return arr[i]; // Clamps to arr[0] = 1
}
// run: test_bounds_negative_index_read() == 1

int test_bounds_upper_bound_read() {
    int arr[3]; arr[0]=1; arr[1]=2; arr[2]=3;
    int i=3; return arr[i]; // Clamps to arr[2] = 3
}
// run: test_bounds_upper_bound_read() == 3

// Similar tests for writes...
```

## Validation

```bash
scripts/filetests.sh array/phase/2-bounds-checking.glsl
```

Expected: All 10 tests pass.

## Code Quality Notes

- Place `lower_array.rs` with other lowering modules
- Export from `lib.rs`
- The clamping logic uses 3 comparisons (lt zero, ge length) and 2 selects
- Future optimization: merge clamping checks or use saturating arithmetic if available
- Add comment explaining clamping behavior is v1, trapping planned for future
