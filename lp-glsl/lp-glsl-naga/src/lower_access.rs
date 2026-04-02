//! Dynamic `Expression::Access` (subscript) lowering: select chains and flat-index merge stores.

use alloc::format;
use alloc::string::String;
use lpir::{IrType, Op, VReg};
use naga::{Expression, Handle, Scalar, TypeInner, VectorSize};

use crate::lower_ctx::{LowerCtx, VRegVec, naga_scalar_to_ir_type, vector_size_usize};
use crate::lower_error::LowerError;
use crate::lower_expr::coerce_assignment_vregs;

/// `v[index]` / `m[index]` column: pick one lane from `n` registers (index is I32).
pub(crate) fn select_lane_dynamic(
    ctx: &mut LowerCtx<'_>,
    lanes: &[VReg],
    index_i32: VReg,
    lane_ty: IrType,
) -> Result<VReg, LowerError> {
    let n = lanes.len();
    if n == 0 {
        return Err(LowerError::Internal(String::from(
            "select_lane_dynamic: empty",
        )));
    }
    if n == 1 {
        return Ok(lanes[0]);
    }
    let mut acc = lanes[n - 1];
    for j in (0..n - 1).rev() {
        let j_imm = ctx.fb.alloc_vreg(IrType::I32);
        ctx.fb.push(Op::IconstI32 {
            dst: j_imm,
            value: j as i32,
        });
        let eq = ctx.fb.alloc_vreg(IrType::I32);
        ctx.fb.push(Op::Ieq {
            dst: eq,
            lhs: index_i32,
            rhs: j_imm,
        });
        let out = ctx.fb.alloc_vreg(lane_ty);
        ctx.fb.push(Op::Select {
            dst: out,
            cond: eq,
            if_true: lanes[j],
            if_false: acc,
        });
        acc = out;
    }
    Ok(acc)
}

/// Write `new_val` into `cells[flat_v]` when `flat_v` is in range (merge via per-cell select).
fn merge_flat_index_store(
    ctx: &mut LowerCtx<'_>,
    cells: &[VReg],
    flat_v: VReg,
    new_val: VReg,
    lane_ty: IrType,
) -> Result<(), LowerError> {
    for k in 0..cells.len() {
        let eq = ctx.fb.alloc_vreg(IrType::I32);
        ctx.fb.push(Op::IeqImm {
            dst: eq,
            src: flat_v,
            imm: k as i32,
        });
        let merged = ctx.fb.alloc_vreg(lane_ty);
        ctx.fb.push(Op::Select {
            dst: merged,
            cond: eq,
            if_true: new_val,
            if_false: cells[k],
        });
        ctx.fb.push(Op::Copy {
            dst: cells[k],
            src: merged,
        });
    }
    Ok(())
}

/// `a[j] = scalar` for a vector local (dynamic `j`).
fn merge_vector_lane_store(
    ctx: &mut LowerCtx<'_>,
    dsts: &[VReg],
    index_i32: VReg,
    new_val: VReg,
    lane_ty: IrType,
) -> Result<(), LowerError> {
    merge_flat_index_store(ctx, dsts, index_i32, new_val, lane_ty)
}

/// Column of a matrix in flat column-major layout: `m[col][row]` for fixed `row`, varying `col`.
fn matrix_column_pick_row(
    ctx: &mut LowerCtx<'_>,
    flat: &[VReg],
    col_v: VReg,
    ncols: usize,
    nrows: usize,
    row: usize,
    lane_ty: IrType,
) -> Result<VReg, LowerError> {
    if ncols == 0 || nrows == 0 || row >= nrows {
        return Err(LowerError::Internal(String::from(
            "matrix_column_pick_row: bad dims",
        )));
    }
    let mut acc = flat[(ncols - 1) * nrows + row];
    for c in (0..ncols - 1).rev() {
        let flat_i = c * nrows + row;
        let c_imm = ctx.fb.alloc_vreg(IrType::I32);
        ctx.fb.push(Op::IconstI32 {
            dst: c_imm,
            value: c as i32,
        });
        let eq = ctx.fb.alloc_vreg(IrType::I32);
        ctx.fb.push(Op::Ieq {
            dst: eq,
            lhs: col_v,
            rhs: c_imm,
        });
        let out = ctx.fb.alloc_vreg(lane_ty);
        ctx.fb.push(Op::Select {
            dst: out,
            cond: eq,
            if_true: flat[flat_i],
            if_false: acc,
        });
        acc = out;
    }
    Ok(acc)
}

/// Load matrix column as `nrows` values (dynamic column index).
pub(crate) fn matrix_column_dynamic(
    ctx: &mut LowerCtx<'_>,
    flat: &[VReg],
    col_v: VReg,
    columns: VectorSize,
    rows: VectorSize,
    scalar: Scalar,
) -> Result<VRegVec, LowerError> {
    let ncols = vector_size_usize(columns);
    let nrows = vector_size_usize(rows);
    let lane_ty = naga_scalar_to_ir_type(scalar.kind)?;
    let mut col = VRegVec::new();
    for r in 0..nrows {
        let v = matrix_column_pick_row(ctx, flat, col_v, ncols, nrows, r, lane_ty)?;
        col.push(v);
    }
    Ok(col)
}

/// Store a column vector into `flat` at dynamic column index `col_v`.
fn store_matrix_column_dynamic(
    ctx: &mut LowerCtx<'_>,
    flat: &[VReg],
    col_v: VReg,
    nrows: usize,
    srcs: &[VReg],
    lane_ty: IrType,
) -> Result<(), LowerError> {
    if srcs.len() != nrows {
        return Err(LowerError::UnsupportedStatement(format!(
            "matrix column store: expected {nrows} components, got {}",
            srcs.len()
        )));
    }
    let nrows_imm = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(Op::IconstI32 {
        dst: nrows_imm,
        value: nrows as i32,
    });
    for r in 0..nrows {
        let mul = ctx.fb.alloc_vreg(IrType::I32);
        ctx.fb.push(Op::Imul {
            dst: mul,
            lhs: col_v,
            rhs: nrows_imm,
        });
        let r_imm = ctx.fb.alloc_vreg(IrType::I32);
        ctx.fb.push(Op::IconstI32 {
            dst: r_imm,
            value: r as i32,
        });
        let flat_v = ctx.fb.alloc_vreg(IrType::I32);
        ctx.fb.push(Op::Iadd {
            dst: flat_v,
            lhs: mul,
            rhs: r_imm,
        });
        merge_flat_index_store(ctx, flat, flat_v, srcs[r], lane_ty)?;
    }
    Ok(())
}

/// Matrix element `m[col][row]` with dynamic indices: `flat[col * nrows + row] = val`.
fn store_matrix_element_dynamic(
    ctx: &mut LowerCtx<'_>,
    flat: &[VReg],
    col_v: VReg,
    row_v: VReg,
    nrows: usize,
    new_val: VReg,
    lane_ty: IrType,
) -> Result<(), LowerError> {
    let nrows_imm = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(Op::IconstI32 {
        dst: nrows_imm,
        value: nrows as i32,
    });
    let mul = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(Op::Imul {
        dst: mul,
        lhs: col_v,
        rhs: nrows_imm,
    });
    let flat_v = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(Op::Iadd {
        dst: flat_v,
        lhs: mul,
        rhs: row_v,
    });
    merge_flat_index_store(ctx, flat, flat_v, new_val, lane_ty)
}

fn load_inout_vector_lanes(
    ctx: &mut LowerCtx<'_>,
    arg_i: u32,
    n: usize,
    scalar: Scalar,
) -> Result<VRegVec, LowerError> {
    let lane_ty = naga_scalar_to_ir_type(scalar.kind)?;
    let addr = ctx.arg_vregs_for(arg_i)?[0];
    let mut v = VRegVec::new();
    for j in 0..n {
        let dst = ctx.fb.alloc_vreg(lane_ty);
        ctx.fb.push(Op::Load {
            dst,
            base: addr,
            offset: (j * 4) as u32,
        });
        v.push(dst);
    }
    Ok(v)
}

fn store_inout_vector_lanes(
    ctx: &mut LowerCtx<'_>,
    arg_i: u32,
    lanes: &[VReg],
) -> Result<(), LowerError> {
    let addr = ctx.arg_vregs_for(arg_i)?[0];
    for (j, &val) in lanes.iter().enumerate() {
        ctx.fb.push(Op::Store {
            base: addr,
            offset: (j * 4) as u32,
            value: val,
        });
    }
    Ok(())
}

/// R-value: result of `Access { base, index }`.
pub(crate) fn lower_access_expr_vec(
    ctx: &mut LowerCtx<'_>,
    access_h: Handle<Expression>,
) -> Result<VRegVec, LowerError> {
    // Try mixed Access/AccessIndex chain first for multi-dimensional arrays.
    if let Some((root, ops)) =
        crate::lower_array_multidim::peel_array_subscript_chain(ctx.func, access_h)
    {
        if let Some(info) = ctx.array_info_for_subscript_root(root)? {
            if ops.len() == info.dimensions.len() {
                let flat_v = crate::lower_array::emit_row_major_flat_from_operands(
                    ctx,
                    &info.dimensions,
                    &ops,
                )?;
                return crate::lower_array::load_array_element_dynamic(ctx, &info, flat_v);
            }
        }
    }
    // Try pure Access chain (for backwards compatibility).
    if let Some((lv, idx_handles)) =
        crate::lower_array_multidim::peel_access_chain(ctx.func, access_h)
    {
        if let Some(info) = ctx.array_map.get(&lv).cloned() {
            if idx_handles.len() == info.dimensions.len() {
                let mut vregs = alloc::vec::Vec::new();
                for &h in &idx_handles {
                    vregs.push(ctx.ensure_expr(h)?);
                }
                let flat = crate::lower_array::emit_row_major_flat_index_vregs(
                    ctx,
                    &info.dimensions,
                    &vregs,
                )?;
                return crate::lower_array::load_array_element_dynamic(ctx, &info, flat);
            }
        }
    }
    let Expression::Access { base, index } = &ctx.func.expressions[access_h] else {
        return Err(LowerError::Internal(String::from(
            "lower_access_expr_vec: not Access",
        )));
    };
    let index_v = ctx.ensure_expr(*index)?;
    match &ctx.func.expressions[*base] {
        Expression::LocalVariable(lv) => {
            if let Some(info) = ctx.array_map.get(lv).cloned() {
                return crate::lower_array::load_array_element_dynamic(ctx, &info, index_v);
            }
            let inner = &ctx.module.types[ctx.func.local_variables[*lv].ty].inner;
            match *inner {
                TypeInner::Vector { scalar, .. } => {
                    let vs = ctx.resolve_local(*lv)?;
                    let t = naga_scalar_to_ir_type(scalar.kind)?;
                    let out = select_lane_dynamic(ctx, &vs, index_v, t)?;
                    Ok(smallvec::smallvec![out])
                }
                TypeInner::Matrix {
                    columns,
                    rows,
                    scalar,
                } => {
                    let vs = ctx.resolve_local(*lv)?;
                    matrix_column_dynamic(ctx, &vs, index_v, columns, rows, scalar)
                }
                _ => Err(LowerError::UnsupportedExpression(format!(
                    "Access on unsupported local type {inner:?}"
                ))),
            }
        }
        Expression::FunctionArgument(arg_i) if ctx.pointer_args.contains_key(arg_i) => {
            let pointee = ctx.pointer_args[arg_i];
            let inner = &ctx.module.types[pointee].inner;
            match *inner {
                TypeInner::Vector { size, scalar, .. } => {
                    let n = vector_size_usize(size);
                    let vs = load_inout_vector_lanes(ctx, *arg_i, n, scalar)?;
                    let t = naga_scalar_to_ir_type(scalar.kind)?;
                    let out = select_lane_dynamic(ctx, &vs, index_v, t)?;
                    Ok(smallvec::smallvec![out])
                }
                TypeInner::Matrix {
                    columns,
                    rows,
                    scalar,
                } => {
                    let n = vector_size_usize(columns) * vector_size_usize(rows);
                    let vs = load_inout_vector_lanes(ctx, *arg_i, n, scalar)?;
                    matrix_column_dynamic(ctx, &vs, index_v, columns, rows, scalar)
                }
                TypeInner::Array { .. } => {
                    let (dimensions, leaf_ty, leaf_stride) =
                        crate::lower_array_multidim::flatten_array_type_shape(ctx.module, pointee)?;
                    let element_count = dimensions
                        .iter()
                        .try_fold(1u32, |acc, &d| acc.checked_mul(d))
                        .ok_or_else(|| {
                            LowerError::Internal(String::from(
                                "Access load: array element count overflow",
                            ))
                        })?;
                    let info = crate::lower_ctx::ArrayInfo {
                        slot: crate::lower_ctx::ArraySlot::Param(*arg_i),
                        dimensions,
                        leaf_element_ty: leaf_ty,
                        leaf_stride,
                        element_count,
                    };
                    crate::lower_array::load_array_element_dynamic(ctx, &info, index_v)
                }
                _ => Err(LowerError::UnsupportedExpression(String::from(
                    "Access on unsupported pointer argument type",
                ))),
            }
        }
        _ => Err(LowerError::UnsupportedExpression(String::from(
            "Access: unsupported base expression",
        ))),
    }
}

/// `Store` with pointer `Access { .. }`.
pub(crate) fn store_through_access(
    ctx: &mut LowerCtx<'_>,
    access_h: Handle<Expression>,
    value: Handle<Expression>,
) -> Result<(), LowerError> {
    // Try mixed Access/AccessIndex chain first for multi-dimensional arrays.
    if let Some((root, ops)) =
        crate::lower_array_multidim::peel_array_subscript_chain(ctx.func, access_h)
    {
        if let Some(info) = ctx.array_info_for_subscript_root(root)? {
            if ops.len() == info.dimensions.len() {
                let flat_v = crate::lower_array::emit_row_major_flat_from_operands(
                    ctx,
                    &info.dimensions,
                    &ops,
                )?;
                return crate::lower_array::store_array_element_dynamic(ctx, &info, flat_v, value);
            }
        }
    }
    // Try pure Access chain (for backwards compatibility).
    if let Some((lv, idx_handles)) =
        crate::lower_array_multidim::peel_access_chain(ctx.func, access_h)
    {
        if let Some(info) = ctx.array_map.get(&lv).cloned() {
            if idx_handles.len() == info.dimensions.len() {
                let mut vregs = alloc::vec::Vec::new();
                for &h in &idx_handles {
                    vregs.push(ctx.ensure_expr(h)?);
                }
                let flat = crate::lower_array::emit_row_major_flat_index_vregs(
                    ctx,
                    &info.dimensions,
                    &vregs,
                )?;
                return crate::lower_array::store_array_element_dynamic(ctx, &info, flat, value);
            }
        }
    }
    let Expression::Access { base, index } = &ctx.func.expressions[access_h] else {
        return Err(LowerError::Internal(String::from(
            "store_through_access: not Access",
        )));
    };
    let index_v = ctx.ensure_expr(*index)?;
    match &ctx.func.expressions[*base] {
        Expression::LocalVariable(lv) => {
            if let Some(info) = ctx.array_map.get(lv).cloned() {
                return crate::lower_array::store_array_element_dynamic(ctx, &info, index_v, value);
            }
            let inner = &ctx.module.types[ctx.func.local_variables[*lv].ty].inner;
            let dsts = ctx.resolve_local(*lv)?;
            match *inner {
                TypeInner::Vector { scalar, .. } => {
                    let scalar_inner = TypeInner::Scalar(scalar);
                    let raw = ctx.ensure_expr_vec(value)?;
                    let srcs = coerce_assignment_vregs(ctx, None, &scalar_inner, value, raw)?;
                    if srcs.len() != 1 {
                        return Err(LowerError::UnsupportedStatement(format!(
                            "vector component store expects 1 value, got {}",
                            srcs.len()
                        )));
                    }
                    let lane_ty = naga_scalar_to_ir_type(scalar.kind)?;
                    merge_vector_lane_store(ctx, &dsts, index_v, srcs[0], lane_ty)?;
                    Ok(())
                }
                TypeInner::Matrix { rows, scalar, .. } => {
                    let nrows = vector_size_usize(rows);
                    let col_ty = TypeInner::Vector { size: rows, scalar };
                    let raw = ctx.ensure_expr_vec(value)?;
                    let srcs = coerce_assignment_vregs(ctx, None, &col_ty, value, raw)?;
                    let lane_ty = naga_scalar_to_ir_type(scalar.kind)?;
                    store_matrix_column_dynamic(ctx, &dsts, index_v, nrows, &srcs, lane_ty)?;
                    Ok(())
                }
                _ => Err(LowerError::UnsupportedStatement(format!(
                    "Access store on unsupported local {inner:?}"
                ))),
            }
        }
        Expression::Access {
            base: mat_base,
            index: col_idx_h,
        } => {
            let Expression::LocalVariable(lv) = &ctx.func.expressions[*mat_base] else {
                return Err(LowerError::UnsupportedStatement(String::from(
                    "matrix element store: inner base must be local matrix",
                )));
            };
            let lv_ty = &ctx.module.types[ctx.func.local_variables[*lv].ty].inner;
            let TypeInner::Matrix {
                columns: _,
                rows,
                scalar,
            } = lv_ty
            else {
                return Err(LowerError::UnsupportedStatement(String::from(
                    "matrix element store: not a matrix local",
                )));
            };
            let nrows = vector_size_usize(*rows);
            let col_v = ctx.ensure_expr(*col_idx_h)?;
            let row_v = index_v;
            let dsts = ctx.resolve_local(*lv)?;
            let scalar_inner = TypeInner::Scalar(*scalar);
            let raw = ctx.ensure_expr_vec(value)?;
            let srcs = coerce_assignment_vregs(ctx, None, &scalar_inner, value, raw)?;
            if srcs.len() != 1 {
                return Err(LowerError::UnsupportedStatement(format!(
                    "matrix element store expects 1 value, got {}",
                    srcs.len()
                )));
            }
            let lane_ty = naga_scalar_to_ir_type(scalar.kind)?;
            store_matrix_element_dynamic(ctx, &dsts, col_v, row_v, nrows, srcs[0], lane_ty)?;
            Ok(())
        }
        Expression::FunctionArgument(arg_i) if ctx.pointer_args.contains_key(arg_i) => {
            let pointee = ctx.pointer_args[arg_i];
            let inner = &ctx.module.types[pointee].inner;
            match *inner {
                TypeInner::Vector { size, scalar, .. } => {
                    let n = vector_size_usize(size);
                    let vs = load_inout_vector_lanes(ctx, *arg_i, n, scalar)?;
                    let scalar_inner = TypeInner::Scalar(scalar);
                    let raw = ctx.ensure_expr_vec(value)?;
                    let srcs = coerce_assignment_vregs(ctx, None, &scalar_inner, value, raw)?;
                    if srcs.len() != 1 {
                        return Err(LowerError::UnsupportedStatement(String::from(
                            "vector pointer store: expected one scalar",
                        )));
                    }
                    let lane_ty = naga_scalar_to_ir_type(scalar.kind)?;
                    merge_vector_lane_store(ctx, &vs, index_v, srcs[0], lane_ty)?;
                    store_inout_vector_lanes(ctx, *arg_i, &vs)?;
                    Ok(())
                }
                TypeInner::Array { .. } => {
                    let (dimensions, leaf_ty, leaf_stride) =
                        crate::lower_array_multidim::flatten_array_type_shape(ctx.module, pointee)?;
                    let element_count = dimensions
                        .iter()
                        .try_fold(1u32, |acc, &d| acc.checked_mul(d))
                        .ok_or_else(|| {
                            LowerError::Internal(String::from(
                                "Access store: array element count overflow",
                            ))
                        })?;
                    let info = crate::lower_ctx::ArrayInfo {
                        slot: crate::lower_ctx::ArraySlot::Param(*arg_i),
                        dimensions,
                        leaf_element_ty: leaf_ty,
                        leaf_stride,
                        element_count,
                    };
                    crate::lower_array::store_array_element_dynamic(ctx, &info, index_v, value)
                }
                _ => Err(LowerError::UnsupportedStatement(String::from(
                    "Access store: unsupported pointer arg pointee for dynamic subscript",
                ))),
            }
        }
        _ => Err(LowerError::UnsupportedStatement(String::from(
            "store through Access: unsupported base",
        ))),
    }
}
