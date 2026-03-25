# Phase 5: Matrix Operations

## Scope

Add `lower_matrix.rs` for matrix-specific operations: matrix × vector,
matrix × matrix, transpose, determinant, inverse. All decompose to
scalar LPIR ops.

## Implementation Details

### `lib.rs` — add module

```rust
pub(crate) mod lower_matrix;
```

### Matrix layout convention

Naga uses **column-major** storage (same as GLSL). A `mat3` has 3
columns, each a `vec3`. Flattened to VRegs:

```
[c0.x, c0.y, c0.z, c1.x, c1.y, c1.z, c2.x, c2.y, c2.z]
```

Indexing helper:

```rust
fn mat_elem(vregs: &[VReg], rows: usize, col: usize, row: usize) -> VReg {
    vregs[col * rows + row]
}
```

### `lower_matrix.rs` — mat × vec

`mat * vec` (matrix on left, vector on right). Result is a vector with
`rows` components. Each component is the dot product of a matrix row
with the vector.

```rust
pub(crate) fn lower_mat_vec_mul(
    ctx: &mut LowerCtx<'_>,
    mat: &[VReg],
    vec: &[VReg],
    cols: usize,
    rows: usize,
) -> Result<VRegVec, LowerError> {
    let mut result = VRegVec::new();
    for r in 0..rows {
        // row r of matrix dot vector
        let mut sum = {
            let d = ctx.fb.alloc_vreg(IrType::F32);
            ctx.fb.push(Op::Fmul {
                dst: d,
                lhs: mat_elem(mat, rows, 0, r),
                rhs: vec[0],
            });
            d
        };
        for c in 1..cols {
            let prod = ctx.fb.alloc_vreg(IrType::F32);
            ctx.fb.push(Op::Fmul {
                dst: prod,
                lhs: mat_elem(mat, rows, c, r),
                rhs: vec[c],
            });
            let next = ctx.fb.alloc_vreg(IrType::F32);
            ctx.fb.push(Op::Fadd { dst: next, lhs: sum, rhs: prod });
            sum = next;
        }
        result.push(sum);
    }
    Ok(result)
}
```

### `lower_matrix.rs` — vec × mat

`vec * mat` (vector on left, matrix on right). Result is a vector with
`cols` components. Each component is the dot product of the vector with
a matrix column.

```rust
pub(crate) fn lower_vec_mat_mul(
    ctx: &mut LowerCtx<'_>,
    vec: &[VReg],
    mat: &[VReg],
    cols: usize,
    rows: usize,
) -> Result<VRegVec, LowerError> {
    let mut result = VRegVec::new();
    for c in 0..cols {
        let col_start = c * rows;
        let col = &mat[col_start..col_start + rows];
        let dot = emit_dot_product(ctx, vec, col)?;
        result.push(dot);
    }
    Ok(result)
}
```

### `lower_matrix.rs` — mat × mat

Each column of the result is `left_mat × right_col`:

```rust
pub(crate) fn lower_mat_mat_mul(
    ctx: &mut LowerCtx<'_>,
    left: &[VReg],
    right: &[VReg],
    left_cols: usize,
    left_rows: usize,
    right_cols: usize,
) -> Result<VRegVec, LowerError> {
    let mut result = VRegVec::new();
    for c in 0..right_cols {
        let right_col_start = c * left_cols;
        let right_col = &right[right_col_start..right_col_start + left_cols];
        let out_col = lower_mat_vec_mul(ctx, left, right_col, left_cols, left_rows)?;
        result.extend_from_slice(&out_col);
    }
    Ok(result)
}
```

### `lower_matrix.rs` — transpose

No arithmetic ops — just rearrange VRegs:

```rust
pub(crate) fn lower_transpose(
    mat: &[VReg],
    cols: usize,
    rows: usize,
) -> VRegVec {
    let mut result = VRegVec::new();
    // Transposed: new_cols = rows, new_rows = cols
    for new_c in 0..rows {
        for new_r in 0..cols {
            // Original element at (col=new_r, row=new_c)
            result.push(mat[new_r * rows + new_c]);
        }
    }
    result
}
```

### `lower_matrix.rs` — determinant

Scalar result. Inline cofactor expansion.

**2×2**:
```
det = a*d - b*c
```

**3×3** (Sarrus' rule):
```
det = a(ei-fh) - b(di-fg) + c(dh-eg)
```

**4×4**: expand by first row using 3×3 cofactors.

```rust
pub(crate) fn lower_determinant(
    ctx: &mut LowerCtx<'_>,
    mat: &[VReg],
    size: usize,
) -> Result<VReg, LowerError> {
    match size {
        2 => det2(ctx, mat),
        3 => det3(ctx, mat),
        4 => det4(ctx, mat),
        _ => Err(LowerError::UnsupportedExpression(...))
    }
}
```

Each `detN` function emits multiply and add/sub ops inline.

### `lower_matrix.rs` — inverse

**2×2**:
```
inv = (1/det) * [[d, -b], [-c, a]]
```

**3×3**: cofactor matrix, transpose, divide by determinant.

**4×4**: cofactor expansion (16 cofactors from 3×3 minors).

```rust
pub(crate) fn lower_inverse(
    ctx: &mut LowerCtx<'_>,
    mat: &[VReg],
    size: usize,
) -> Result<VRegVec, LowerError>
```

This generates many ops but is straightforward inline arithmetic.

### Integration with `lower_expr.rs`

In `lower_expr_vec_uncached`, when processing `Binary { op: Multiply }`
and the types are matrix-involved:

```rust
match (left_inner, right_inner) {
    (TypeInner::Matrix { columns, rows, .. }, TypeInner::Vector { .. }) => {
        let mat_vs = lower_expr_vec(ctx, left)?;
        let vec_vs = lower_expr_vec(ctx, right)?;
        lower_matrix::lower_mat_vec_mul(ctx, &mat_vs, &vec_vs, cols, rows)
    }
    (TypeInner::Vector { .. }, TypeInner::Matrix { columns, rows, .. }) => {
        let vec_vs = lower_expr_vec(ctx, left)?;
        let mat_vs = lower_expr_vec(ctx, right)?;
        lower_matrix::lower_vec_mat_mul(ctx, &vec_vs, &mat_vs, cols, rows)
    }
    (TypeInner::Matrix { .. }, TypeInner::Matrix { .. }) => {
        // mat * mat
        lower_matrix::lower_mat_mat_mul(ctx, &left_vs, &right_vs, ...)
    }
    _ => // component-wise
}
```

For `Math { fun: Transpose/Determinant/Inverse }`, dispatch to
`lower_matrix::lower_transpose/determinant/inverse`.

## Validate

```
cargo test -p lp-glsl-naga
cargo +nightly fmt -p lp-glsl-naga -- --check
cargo clippy -p lp-glsl-naga
```

Matrix filetests (mat*vec, mat*mat, transpose, determinant, inverse)
should now lower.
