//! Dynamic `Expression::Access` (subscript) lowering: select chains and flat-index merge stores.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use lpir::{IrType, LpirOp, VMCTX_VREG, VReg};
use lps_shared::{LayoutRules, LpsType, array_stride};
use naga::{Expression, GlobalVariable, Handle, Scalar, TypeInner, VectorSize};

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
        ctx.fb.push(LpirOp::IconstI32 {
            dst: j_imm,
            value: j as i32,
        });
        let eq = ctx.fb.alloc_vreg(IrType::I32);
        ctx.fb.push(LpirOp::Ieq {
            dst: eq,
            lhs: index_i32,
            rhs: j_imm,
        });
        let out = ctx.fb.alloc_vreg(lane_ty);
        ctx.fb.push(LpirOp::Select {
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
        ctx.fb.push(LpirOp::IeqImm {
            dst: eq,
            src: flat_v,
            imm: k as i32,
        });
        let merged = ctx.fb.alloc_vreg(lane_ty);
        ctx.fb.push(LpirOp::Select {
            dst: merged,
            cond: eq,
            if_true: new_val,
            if_false: cells[k],
        });
        ctx.fb.push(LpirOp::Copy {
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
        ctx.fb.push(LpirOp::IconstI32 {
            dst: c_imm,
            value: c as i32,
        });
        let eq = ctx.fb.alloc_vreg(IrType::I32);
        ctx.fb.push(LpirOp::Ieq {
            dst: eq,
            lhs: col_v,
            rhs: c_imm,
        });
        let out = ctx.fb.alloc_vreg(lane_ty);
        ctx.fb.push(LpirOp::Select {
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
    ctx.fb.push(LpirOp::IconstI32 {
        dst: nrows_imm,
        value: nrows as i32,
    });
    for r in 0..nrows {
        let mul = ctx.fb.alloc_vreg(IrType::I32);
        ctx.fb.push(LpirOp::Imul {
            dst: mul,
            lhs: col_v,
            rhs: nrows_imm,
        });
        let r_imm = ctx.fb.alloc_vreg(IrType::I32);
        ctx.fb.push(LpirOp::IconstI32 {
            dst: r_imm,
            value: r as i32,
        });
        let flat_v = ctx.fb.alloc_vreg(IrType::I32);
        ctx.fb.push(LpirOp::Iadd {
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
    ctx.fb.push(LpirOp::IconstI32 {
        dst: nrows_imm,
        value: nrows as i32,
    });
    let mul = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(LpirOp::Imul {
        dst: mul,
        lhs: col_v,
        rhs: nrows_imm,
    });
    let flat_v = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(LpirOp::Iadd {
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
        ctx.fb.push(LpirOp::Load {
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
        ctx.fb.push(LpirOp::Store {
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
        if let Some(info) = ctx.aggregate_info_for_subscript_root(root)? {
            if matches!(
                &info.layout.kind,
                crate::naga_util::AggregateKind::Array { .. }
            ) && ops.len() == info.dimensions().len()
            {
                let flat_v = crate::lower_array::emit_row_major_flat_from_operands(
                    ctx,
                    &info.dimensions(),
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
        if let Some(info) = ctx.aggregate_map.get(&lv).cloned() {
            if matches!(
                &info.layout.kind,
                crate::naga_util::AggregateKind::Array { .. }
            ) && idx_handles.len() == info.dimensions().len()
            {
                let mut vregs = alloc::vec::Vec::new();
                for &h in &idx_handles {
                    vregs.push(ctx.ensure_expr(h)?);
                }
                let flat = crate::lower_array::emit_row_major_flat_index_vregs(
                    ctx,
                    &info.dimensions(),
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
            if let Some(info) = ctx.aggregate_map.get(lv).cloned() {
                if matches!(
                    &info.layout.kind,
                    crate::naga_util::AggregateKind::Array { .. }
                ) {
                    return crate::lower_array::load_array_element_dynamic(ctx, &info, index_v);
                }
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
                    let layout = crate::naga_util::aggregate_layout(ctx.module, pointee)?
                        .ok_or_else(|| {
                            LowerError::Internal(String::from(
                                "Access load: expected array aggregate layout",
                            ))
                        })?;
                    // `pointer_args` only: `inout`/`out` array pointer, not M5 `ParamReadOnly` (those use
                    // a `LocalVariable` entry in `aggregate_map`).
                    let info = crate::lower_ctx::AggregateInfo {
                        slot: crate::lower_ctx::AggregateSlot::Param(*arg_i),
                        layout,
                        naga_ty: pointee,
                    };
                    crate::lower_array::load_array_element_dynamic(ctx, &info, index_v)
                }
                _ => Err(LowerError::UnsupportedExpression(String::from(
                    "Access on unsupported pointer argument type",
                ))),
            }
        }
        Expression::GlobalVariable(gv) => {
            let Some(info) = ctx.global_map.get(gv).cloned() else {
                return Err(LowerError::Internal(format!(
                    "Access global: {gv:?} not in global_map"
                )));
            };
            if !info.vmctx_backed {
                return Err(LowerError::UnsupportedExpression(String::from(
                    "Access: global has no VMContext layout (internal sampler stub)",
                )));
            }
            let LpsType::Array { element, .. } = info.ty else {
                return Err(LowerError::UnsupportedExpression(String::from(
                    "Access: global base is not an array",
                )));
            };
            let element = *element;
            let byte_offset = info.byte_offset;
            let index_v = ctx.ensure_expr(*index)?;
            let stride = array_stride(&element, LayoutRules::Std430) as u32;
            let stride_i = i32::try_from(stride).map_err(|_| {
                LowerError::Internal(String::from("uniform top-level array: stride"))
            })?;
            let prod = ctx.fb.alloc_vreg(IrType::I32);
            ctx.fb.push(LpirOp::ImulImm {
                dst: prod,
                src: index_v,
                imm: stride_i,
            });
            let off_imm = i32::try_from(byte_offset).map_err(|_| {
                LowerError::Internal(String::from("uniform top-level array: base offset"))
            })?;
            let byte_off = ctx.fb.alloc_vreg(IrType::I32);
            ctx.fb.push(LpirOp::IaddImm {
                dst: byte_off,
                src: prod,
                imm: off_imm,
            });
            let addr = ctx.fb.alloc_vreg(IrType::Pointer);
            ctx.fb.push(LpirOp::Iadd {
                dst: addr,
                lhs: VMCTX_VREG,
                rhs: byte_off,
            });
            crate::lower_expr::load_lps_value_from_vmctx_with_base(ctx, addr, 0, &element)
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
        if let Some(info) = ctx.aggregate_info_for_subscript_root(root)? {
            if matches!(
                &info.layout.kind,
                crate::naga_util::AggregateKind::Array { .. }
            ) && ops.len() == info.dimensions().len()
            {
                let flat_v = crate::lower_array::emit_row_major_flat_from_operands(
                    ctx,
                    &info.dimensions(),
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
        if let Some(info) = ctx.aggregate_map.get(&lv).cloned() {
            if matches!(
                &info.layout.kind,
                crate::naga_util::AggregateKind::Array { .. }
            ) && idx_handles.len() == info.dimensions().len()
            {
                let mut vregs = alloc::vec::Vec::new();
                for &h in &idx_handles {
                    vregs.push(ctx.ensure_expr(h)?);
                }
                let flat = crate::lower_array::emit_row_major_flat_index_vregs(
                    ctx,
                    &info.dimensions(),
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
            if let Some(info) = ctx.aggregate_map.get(lv).cloned() {
                if matches!(
                    &info.layout.kind,
                    crate::naga_util::AggregateKind::Array { .. }
                ) {
                    return crate::lower_array::store_array_element_dynamic(
                        ctx, &info, index_v, value,
                    );
                }
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
                    let layout = crate::naga_util::aggregate_layout(ctx.module, pointee)?
                        .ok_or_else(|| {
                            LowerError::Internal(String::from(
                                "Access store: expected array aggregate layout",
                            ))
                        })?;
                    // `pointer_args` only (see `Access load` array arm above).
                    let info = crate::lower_ctx::AggregateInfo {
                        slot: crate::lower_ctx::AggregateSlot::Param(*arg_i),
                        layout,
                        naga_ty: pointee,
                    };
                    crate::lower_array::store_array_element_dynamic(ctx, &info, index_v, value)
                }
                _ => Err(LowerError::UnsupportedStatement(String::from(
                    "Access store: unsupported pointer arg pointee for dynamic subscript",
                ))),
            }
        }
        Expression::GlobalVariable(gv) => {
            ctx.global_map.get(gv).ok_or_else(|| {
                LowerError::Internal(format!("Access store global: {gv:?} not in global_map"))
            })?;
            uniform_global_must_not_write(ctx, *gv)?;
            let naga_ty = ctx.module.global_variables[*gv].ty;
            let root_inner = gv_root_value_inner_after_ptr(ctx.module, *gv);
            match &root_inner {
                TypeInner::Array { .. } => {
                    let layout = crate::naga_util::aggregate_layout(ctx.module, naga_ty)?
                        .ok_or_else(|| {
                            LowerError::Internal(String::from(
                                "Access store: global subscript — expected array aggregate layout",
                            ))
                        })?;
                    if !matches!(&layout.kind, crate::naga_util::AggregateKind::Array { .. }) {
                        return Err(LowerError::UnsupportedStatement(String::from(
                            "subscript store: global base is not an array",
                        )));
                    }
                    let info = crate::lower_ctx::AggregateInfo {
                        slot: crate::lower_ctx::AggregateSlot::Global(*gv),
                        layout,
                        naga_ty,
                    };
                    crate::lower_array::store_array_element_dynamic(ctx, &info, index_v, value)
                }
                TypeInner::Vector { scalar, .. } => {
                    let off = gv_vmctx_byte_offset(ctx, *gv)?;
                    let dsts = vmctx_load_flat_by_offset_and_inner(ctx, off, &root_inner)?;
                    let scalar_inner = TypeInner::Scalar(*scalar);
                    let raw = ctx.ensure_expr_vec(value)?;
                    let srcs = coerce_assignment_vregs(ctx, None, &scalar_inner, value, raw)?;
                    if srcs.len() != 1 {
                        return Err(LowerError::UnsupportedStatement(format!(
                            "global vector lane store expects 1 value, got {}",
                            srcs.len()
                        )));
                    }
                    let lane_ty = naga_scalar_to_ir_type(scalar.kind)?;
                    merge_vector_lane_store(ctx, &dsts, index_v, srcs[0], lane_ty)?;
                    vmctx_store_flat_by_offset_and_inner(ctx, off, &root_inner, &dsts)
                }
                TypeInner::Matrix {
                    columns,
                    rows,
                    scalar,
                } => {
                    let ncols = vector_size_usize(*columns);
                    let nrows = vector_size_usize(*rows);
                    let col_ty = TypeInner::Vector {
                        size: *rows,
                        scalar: *scalar,
                    };
                    let raw = ctx.ensure_expr_vec(value)?;
                    let srcs = coerce_assignment_vregs(ctx, None, &col_ty, value, raw)?;
                    if srcs.len() != nrows {
                        return Err(LowerError::UnsupportedStatement(format!(
                            "global matrix column store expects {nrows} values (mat {ncols}x{nrows}), got {}",
                            srcs.len()
                        )));
                    }
                    let lane_ty = naga_scalar_to_ir_type(scalar.kind)?;
                    let off = gv_vmctx_byte_offset(ctx, *gv)?;
                    let dsts = vmctx_load_flat_by_offset_and_inner(ctx, off, &root_inner)?;
                    store_matrix_column_dynamic(ctx, &dsts, index_v, nrows, &srcs, lane_ty)?;
                    vmctx_store_flat_by_offset_and_inner(ctx, off, &root_inner, &dsts)
                }
                _ => Err(LowerError::UnsupportedStatement(String::from(
                    "Access store through global: unsupported value type for subscript",
                ))),
            }
        }
        _ => Err(LowerError::UnsupportedStatement(String::from(
            "store through Access: unsupported base",
        ))),
    }
}

/// Write lowered registers back through a local-only `Access` / `AccessIndex` / single-component
/// `Swizzle` leaf (vector/matrix stack locals). Used by M9 `out` / `inout` temp writeback.
///
/// Does not handle array/struct aggregates, pointer-argument roots, or globals.
pub(crate) fn store_vregs_through_local_access_leaf(
    ctx: &mut LowerCtx<'_>,
    pointer: Handle<Expression>,
    pointee_inner: &TypeInner,
    srcs: &[VReg],
) -> Result<(), LowerError> {
    let expect = crate::lower_ctx::naga_type_to_ir_types(ctx.module, pointee_inner)?.len();
    if srcs.len() != expect {
        return Err(LowerError::Internal(format!(
            "local access writeback: expected {expect} vregs, got {}",
            srcs.len()
        )));
    }
    match &ctx.func.expressions[pointer] {
        Expression::Swizzle {
            size,
            vector,
            pattern,
        } => {
            let n = vector_size_usize(*size);
            if n != 1 {
                return Err(LowerError::UnsupportedExpression(String::from(
                    "inout/out swizzle writeback: expected a single component",
                )));
            }
            let Expression::LocalVariable(lv) = &ctx.func.expressions[*vector] else {
                return Err(LowerError::UnsupportedExpression(String::from(
                    "inout/out swizzle writeback: expected local vector",
                )));
            };
            if ctx.aggregate_map.contains_key(lv) {
                return Err(LowerError::UnsupportedExpression(String::from(
                    "inout/out swizzle writeback: aggregate locals are not supported in this phase",
                )));
            }
            let comp = pattern[0] as usize;
            let dsts = ctx.resolve_local(*lv)?;
            let lv_ty = &ctx.module.types[ctx.func.local_variables[*lv].ty].inner;
            let TypeInner::Vector { .. } = lv_ty else {
                return Err(LowerError::UnsupportedExpression(String::from(
                    "inout/out swizzle writeback: base is not a vector",
                )));
            };
            if comp >= dsts.len() {
                return Err(LowerError::UnsupportedStatement(format!(
                    "swizzle writeback: component {comp} out of range (len {})",
                    dsts.len()
                )));
            }
            ctx.fb.push(LpirOp::Copy {
                dst: dsts[comp],
                src: srcs[0],
            });
            Ok(())
        }
        Expression::AccessIndex { base, index } => match &ctx.func.expressions[*base] {
            Expression::LocalVariable(lv) => {
                if ctx.aggregate_map.contains_key(lv) {
                    return Err(LowerError::UnsupportedExpression(String::from(
                        "inout/out access writeback: aggregate locals are not supported in this phase",
                    )));
                }
                let dsts = ctx.resolve_local(*lv)?;
                let lv_ty = &ctx.module.types[ctx.func.local_variables[*lv].ty].inner;
                match lv_ty {
                    TypeInner::Vector { scalar, .. } => {
                        let comp = *index as usize;
                        if comp >= dsts.len() {
                            return Err(LowerError::UnsupportedStatement(format!(
                                "AccessIndex {comp} out of range (len {})",
                                dsts.len()
                            )));
                        }
                        if srcs.len() != 1 {
                            return Err(LowerError::UnsupportedStatement(String::from(
                                "vector component writeback expects one scalar",
                            )));
                        }
                        let scalar_inner = TypeInner::Scalar(*scalar);
                        if pointee_inner != &scalar_inner {
                            return Err(LowerError::UnsupportedExpression(String::from(
                                "vector lane writeback: pointee type mismatch",
                            )));
                        }
                        ctx.fb.push(LpirOp::Copy {
                            dst: dsts[comp],
                            src: srcs[0],
                        });
                        Ok(())
                    }
                    TypeInner::Matrix {
                        columns,
                        rows,
                        scalar,
                    } => {
                        let ncols = vector_size_usize(*columns);
                        let nrows = vector_size_usize(*rows);
                        let col = *index as usize;
                        if col >= ncols {
                            return Err(LowerError::UnsupportedStatement(format!(
                                "matrix column AccessIndex {col} out of range (cols {ncols})"
                            )));
                        }
                        let col_ty = TypeInner::Vector {
                            size: *rows,
                            scalar: *scalar,
                        };
                        if pointee_inner != &col_ty {
                            return Err(LowerError::UnsupportedExpression(String::from(
                                "matrix column writeback: pointee type mismatch",
                            )));
                        }
                        if srcs.len() != nrows {
                            return Err(LowerError::UnsupportedStatement(format!(
                                "matrix column writeback: expected {nrows} components, got {}",
                                srcs.len()
                            )));
                        }
                        for r in 0..nrows {
                            let flat_i = col * nrows + r;
                            ctx.fb.push(LpirOp::Copy {
                                dst: dsts[flat_i],
                                src: srcs[r],
                            });
                        }
                        Ok(())
                    }
                    _ => Err(LowerError::UnsupportedExpression(String::from(
                        "AccessIndex writeback on non-vector non-matrix local",
                    ))),
                }
            }
            Expression::AccessIndex {
                base: col_base,
                index: col_idx,
            } => {
                let Expression::LocalVariable(lv) = &ctx.func.expressions[*col_base] else {
                    return Err(LowerError::UnsupportedStatement(String::from(
                        "matrix element writeback: expected local matrix",
                    )));
                };
                if ctx.aggregate_map.contains_key(lv) {
                    return Err(LowerError::UnsupportedExpression(String::from(
                        "inout/out matrix cell writeback: aggregate locals are not supported in this phase",
                    )));
                }
                let lv_ty = &ctx.module.types[ctx.func.local_variables[*lv].ty].inner;
                let TypeInner::Matrix {
                    columns,
                    rows,
                    scalar,
                } = lv_ty
                else {
                    return Err(LowerError::UnsupportedStatement(String::from(
                        "nested AccessIndex writeback base is not a matrix local",
                    )));
                };
                let nrows = vector_size_usize(*rows);
                let ncols = vector_size_usize(*columns);
                let col = *col_idx as usize;
                let row = *index as usize;
                if col >= ncols || row >= nrows {
                    return Err(LowerError::UnsupportedStatement(format!(
                        "matrix writeback index out of range col {col} row {row} (mat {ncols}x{nrows})"
                    )));
                }
                let flat_i = col * nrows + row;
                let dsts = ctx.resolve_local(*lv)?;
                if flat_i >= dsts.len() {
                    return Err(LowerError::UnsupportedStatement(format!(
                        "matrix flat index {flat_i} out of range (len {})",
                        dsts.len()
                    )));
                }
                let scalar_inner = TypeInner::Scalar(*scalar);
                if pointee_inner != &scalar_inner {
                    return Err(LowerError::UnsupportedExpression(String::from(
                        "matrix cell writeback: pointee type mismatch",
                    )));
                }
                if srcs.len() != 1 {
                    return Err(LowerError::UnsupportedStatement(String::from(
                        "matrix element writeback expects one scalar",
                    )));
                }
                ctx.fb.push(LpirOp::Copy {
                    dst: dsts[flat_i],
                    src: srcs[0],
                });
                Ok(())
            }
            _ => Err(LowerError::UnsupportedExpression(String::from(
                "AccessIndex writeback: unsupported base",
            ))),
        },
        Expression::Access { base, index } => {
            let index_v = ctx.ensure_expr(*index)?;
            match &ctx.func.expressions[*base] {
                Expression::LocalVariable(lv) => {
                    if ctx.aggregate_map.contains_key(lv) {
                        return Err(LowerError::UnsupportedExpression(String::from(
                            "inout/out access writeback: aggregate locals are not supported in this phase",
                        )));
                    }
                    let inner = &ctx.module.types[ctx.func.local_variables[*lv].ty].inner;
                    let dsts = ctx.resolve_local(*lv)?;
                    match *inner {
                        TypeInner::Vector { scalar, .. } => {
                            let scalar_inner = TypeInner::Scalar(scalar);
                            if pointee_inner != &scalar_inner {
                                return Err(LowerError::UnsupportedExpression(String::from(
                                    "dynamic vector lane writeback: pointee type mismatch",
                                )));
                            }
                            if srcs.len() != 1 {
                                return Err(LowerError::UnsupportedStatement(String::from(
                                    "dynamic vector lane writeback expects one scalar",
                                )));
                            }
                            let lane_ty = naga_scalar_to_ir_type(scalar.kind)?;
                            merge_vector_lane_store(ctx, &dsts, index_v, srcs[0], lane_ty)?;
                            Ok(())
                        }
                        TypeInner::Matrix {
                            columns: _,
                            rows,
                            scalar,
                        } => {
                            let nrows = vector_size_usize(rows);
                            let col_ty = TypeInner::Vector { size: rows, scalar };
                            if pointee_inner != &col_ty {
                                return Err(LowerError::UnsupportedExpression(String::from(
                                    "dynamic matrix column writeback: pointee type mismatch",
                                )));
                            }
                            let lane_ty = naga_scalar_to_ir_type(scalar.kind)?;
                            store_matrix_column_dynamic(ctx, &dsts, index_v, nrows, srcs, lane_ty)?;
                            Ok(())
                        }
                        _ => Err(LowerError::UnsupportedExpression(String::from(
                            "Access writeback on unsupported local type",
                        ))),
                    }
                }
                Expression::Access {
                    base: mat_base,
                    index: col_idx_h,
                } => {
                    let Expression::LocalVariable(lv) = &ctx.func.expressions[*mat_base] else {
                        return Err(LowerError::UnsupportedStatement(String::from(
                            "matrix element writeback: inner base must be local matrix",
                        )));
                    };
                    if ctx.aggregate_map.contains_key(lv) {
                        return Err(LowerError::UnsupportedExpression(String::from(
                            "inout/out matrix cell writeback: aggregate locals are not supported in this phase",
                        )));
                    }
                    let lv_ty = &ctx.module.types[ctx.func.local_variables[*lv].ty].inner;
                    let TypeInner::Matrix {
                        columns: _,
                        rows,
                        scalar,
                    } = lv_ty
                    else {
                        return Err(LowerError::UnsupportedStatement(String::from(
                            "matrix element writeback: not a matrix local",
                        )));
                    };
                    let nrows = vector_size_usize(*rows);
                    let col_v = ctx.ensure_expr(*col_idx_h)?;
                    let row_v = index_v;
                    let scalar_inner = TypeInner::Scalar(*scalar);
                    if pointee_inner != &scalar_inner {
                        return Err(LowerError::UnsupportedExpression(String::from(
                            "dynamic matrix cell writeback: pointee type mismatch",
                        )));
                    }
                    if srcs.len() != 1 {
                        return Err(LowerError::UnsupportedStatement(String::from(
                            "matrix element writeback expects one scalar",
                        )));
                    }
                    let dsts = ctx.resolve_local(*lv)?;
                    let lane_ty = naga_scalar_to_ir_type(scalar.kind)?;
                    store_matrix_element_dynamic(
                        ctx, &dsts, col_v, row_v, nrows, srcs[0], lane_ty,
                    )?;
                    Ok(())
                }
                _ => Err(LowerError::UnsupportedExpression(String::from(
                    "Access writeback: unsupported base",
                ))),
            }
        }
        _ => Err(LowerError::UnsupportedExpression(String::from(
            "inout/out access writeback: unsupported expression shape",
        ))),
    }
}

/// Temp writeback target for callee `out` / `inout`: pointer-parameter vector/matrix accesses.
pub(crate) fn store_vregs_through_pointer_arg_access_leaf(
    ctx: &mut LowerCtx<'_>,
    pointer: Handle<Expression>,
    pointee_inner: &TypeInner,
    srcs: &[VReg],
) -> Result<(), LowerError> {
    let expect = crate::lower_ctx::naga_type_to_ir_types(ctx.module, pointee_inner)?.len();
    if srcs.len() != expect {
        return Err(LowerError::Internal(format!(
            "pointer-arg access writeback: expected {expect} vregs, got {}",
            srcs.len()
        )));
    }
    match &ctx.func.expressions[pointer] {
        Expression::Swizzle {
            size,
            vector,
            pattern,
        } => {
            let n = vector_size_usize(*size);
            if n != 1 {
                return Err(LowerError::UnsupportedExpression(String::from(
                    "pointer-arg swizzle writeback: expected a single component",
                )));
            }
            let Expression::FunctionArgument(arg_i) = &ctx.func.expressions[*vector] else {
                return Err(LowerError::UnsupportedExpression(String::from(
                    "pointer-arg swizzle writeback: expected formal pointer vector",
                )));
            };
            if ctx.pointer_args.get(arg_i).is_none() {
                return Err(LowerError::UnsupportedExpression(String::from(
                    "pointer-arg swizzle writeback: not a pointer formal",
                )));
            }
            let pointee = ctx.pointer_args[arg_i];
            let TypeInner::Vector {
                size: vsz, scalar, ..
            } = &ctx.module.types[pointee].inner
            else {
                return Err(LowerError::UnsupportedExpression(String::from(
                    "pointer-arg swizzle writeback: pointee must be vector",
                )));
            };
            let lane_n = vector_size_usize(*vsz);
            let comp = pattern[0] as usize;
            if comp >= lane_n {
                return Err(LowerError::UnsupportedStatement(format!(
                    "pointer-arg swizzle: component {comp} out of range (len {lane_n})",
                )));
            }
            let dsts = load_inout_vector_lanes(ctx, *arg_i, lane_n, *scalar)?;
            ctx.fb.push(LpirOp::Copy {
                dst: dsts[comp],
                src: srcs[0],
            });
            store_inout_vector_lanes(ctx, *arg_i, &dsts)
        }
        Expression::AccessIndex { base, index } => match &ctx.func.expressions[*base] {
            Expression::FunctionArgument(arg_i) if ctx.pointer_args.contains_key(arg_i) => {
                let pointee = ctx.pointer_args[arg_i];
                let inner = &ctx.module.types[pointee].inner;
                match *inner {
                    TypeInner::Vector { size, scalar, .. } => {
                        let lane_n = vector_size_usize(size);
                        let comp = *index as usize;
                        if comp >= lane_n {
                            return Err(LowerError::UnsupportedStatement(format!(
                                "pointer-arg AccessIndex {comp} out of range (len {lane_n})",
                            )));
                        }
                        if srcs.len() != 1 {
                            return Err(LowerError::UnsupportedStatement(String::from(
                                "pointer-arg vector component writeback expects one scalar",
                            )));
                        }
                        let scalar_inner = TypeInner::Scalar(scalar);
                        if pointee_inner != &scalar_inner {
                            return Err(LowerError::UnsupportedExpression(String::from(
                                "pointer-arg vector lane writeback: pointee type mismatch",
                            )));
                        }
                        let dsts = load_inout_vector_lanes(ctx, *arg_i, lane_n, scalar)?;
                        ctx.fb.push(LpirOp::Copy {
                            dst: dsts[comp],
                            src: srcs[0],
                        });
                        store_inout_vector_lanes(ctx, *arg_i, &dsts)
                    }
                    TypeInner::Matrix {
                        columns,
                        rows,
                        scalar,
                    } => {
                        let ncols = vector_size_usize(columns);
                        let nrows = vector_size_usize(rows);
                        let col = *index as usize;
                        if col >= ncols {
                            return Err(LowerError::UnsupportedStatement(format!(
                                "pointer-arg matrix column AccessIndex {col} out of range (cols {ncols})",
                            )));
                        }
                        let col_ty = TypeInner::Vector { size: rows, scalar };
                        if pointee_inner != &col_ty {
                            return Err(LowerError::UnsupportedExpression(String::from(
                                "pointer-arg matrix column writeback: pointee type mismatch",
                            )));
                        }
                        if srcs.len() != nrows {
                            return Err(LowerError::UnsupportedStatement(format!(
                                "pointer-arg matrix column writeback: expected {nrows} components, got {}",
                                srcs.len()
                            )));
                        }
                        let total = ncols * nrows;
                        let dsts = load_inout_vector_lanes(ctx, *arg_i, total, scalar)?;
                        for r in 0..nrows {
                            let flat_i = col * nrows + r;
                            ctx.fb.push(LpirOp::Copy {
                                dst: dsts[flat_i],
                                src: srcs[r],
                            });
                        }
                        store_inout_vector_lanes(ctx, *arg_i, &dsts)
                    }
                    _ => Err(LowerError::UnsupportedExpression(String::from(
                        "pointer-arg AccessIndex writeback on non-vector non-matrix formal",
                    ))),
                }
            }
            Expression::AccessIndex {
                base: col_base,
                index: col_idx,
            } => {
                let Expression::FunctionArgument(arg_i) = &ctx.func.expressions[*col_base] else {
                    return Err(LowerError::UnsupportedStatement(String::from(
                        "pointer-arg matrix element writeback: expected matrix formal",
                    )));
                };
                if !ctx.pointer_args.contains_key(arg_i) {
                    return Err(LowerError::UnsupportedExpression(String::from(
                        "pointer-arg matrix cell: not a pointer formal",
                    )));
                }
                let pointee = ctx.pointer_args[arg_i];
                let TypeInner::Matrix {
                    columns,
                    rows,
                    scalar,
                } = &ctx.module.types[pointee].inner
                else {
                    return Err(LowerError::UnsupportedStatement(String::from(
                        "pointer-arg nested AccessIndex: not inout matrix",
                    )));
                };
                let nrows = vector_size_usize(*rows);
                let ncols = vector_size_usize(*columns);
                let col = *col_idx as usize;
                let row = *index as usize;
                if col >= ncols || row >= nrows {
                    return Err(LowerError::UnsupportedStatement(format!(
                        "pointer-arg matrix cell index col {col} row {row} ({ncols}x{nrows})",
                    )));
                }
                let flat_i = col * nrows + row;
                let total = ncols * nrows;
                let scalar_inner = TypeInner::Scalar(*scalar);
                if pointee_inner != &scalar_inner {
                    return Err(LowerError::UnsupportedExpression(String::from(
                        "pointer-arg matrix cell writeback: pointee type mismatch",
                    )));
                }
                if srcs.len() != 1 {
                    return Err(LowerError::UnsupportedStatement(String::from(
                        "pointer-arg matrix element writeback expects one scalar",
                    )));
                }
                let dsts = load_inout_vector_lanes(ctx, *arg_i, total, *scalar)?;
                ctx.fb.push(LpirOp::Copy {
                    dst: dsts[flat_i],
                    src: srcs[0],
                });
                store_inout_vector_lanes(ctx, *arg_i, &dsts)
            }
            _ => Err(LowerError::UnsupportedExpression(String::from(
                "pointer-arg AccessIndex writeback: unsupported base",
            ))),
        },
        Expression::Access { base, index } => {
            let index_v = ctx.ensure_expr(*index)?;
            match &ctx.func.expressions[*base] {
                Expression::FunctionArgument(arg_i) if ctx.pointer_args.contains_key(arg_i) => {
                    let pointee = ctx.pointer_args[arg_i];
                    let inner = &ctx.module.types[pointee].inner;
                    match inner {
                        TypeInner::Vector { size, scalar, .. } => {
                            let lane_n = vector_size_usize(*size);
                            let scalar_inner = TypeInner::Scalar(*scalar);
                            if pointee_inner != &scalar_inner {
                                return Err(LowerError::UnsupportedExpression(String::from(
                                    "pointer-arg dynamic vector lane writeback: pointee type mismatch",
                                )));
                            }
                            if srcs.len() != 1 {
                                return Err(LowerError::UnsupportedStatement(String::from(
                                    "pointer-arg dynamic vector lane writeback expects one scalar",
                                )));
                            }
                            let lane_ty = naga_scalar_to_ir_type(scalar.kind)?;
                            let dsts = load_inout_vector_lanes(ctx, *arg_i, lane_n, *scalar)?;
                            merge_vector_lane_store(ctx, &dsts, index_v, srcs[0], lane_ty)?;
                            store_inout_vector_lanes(ctx, *arg_i, &dsts)
                        }
                        TypeInner::Matrix {
                            columns,
                            rows,
                            scalar,
                        } => {
                            let nrows = vector_size_usize(*rows);
                            let ncols = vector_size_usize(*columns);
                            let col_ty = TypeInner::Vector {
                                size: *rows,
                                scalar: *scalar,
                            };
                            if pointee_inner != &col_ty {
                                return Err(LowerError::UnsupportedExpression(String::from(
                                    "pointer-arg dynamic matrix column writeback: pointee type mismatch",
                                )));
                            }
                            let lane_ty = naga_scalar_to_ir_type(scalar.kind)?;
                            let total = ncols * nrows;
                            let dsts = load_inout_vector_lanes(ctx, *arg_i, total, *scalar)?;
                            store_matrix_column_dynamic(ctx, &dsts, index_v, nrows, srcs, lane_ty)?;
                            store_inout_vector_lanes(ctx, *arg_i, &dsts)
                        }
                        _ => Err(LowerError::UnsupportedExpression(String::from(
                            "pointer-arg Access writeback on unsupported formal type",
                        ))),
                    }
                }
                Expression::Access {
                    base: mat_base,
                    index: col_idx_h,
                } => {
                    let Expression::FunctionArgument(arg_i) = &ctx.func.expressions[*mat_base]
                    else {
                        return Err(LowerError::UnsupportedStatement(String::from(
                            "pointer-arg matrix element Access: inner formal must be pointer matrix",
                        )));
                    };
                    if !ctx.pointer_args.contains_key(arg_i) {
                        return Err(LowerError::UnsupportedExpression(String::from(
                            "pointer-arg dynamic matrix cell: not a pointer formal",
                        )));
                    }
                    let pointee = ctx.pointer_args[arg_i];
                    let TypeInner::Matrix {
                        columns,
                        rows,
                        scalar,
                    } = &ctx.module.types[pointee].inner
                    else {
                        return Err(LowerError::UnsupportedStatement(String::from(
                            "pointer-arg matrix element Access inner not matrix formal",
                        )));
                    };
                    let nrows = vector_size_usize(*rows);
                    let total = vector_size_usize(*columns) * nrows;
                    let col_v = ctx.ensure_expr(*col_idx_h)?;
                    let scalar_inner = TypeInner::Scalar(*scalar);
                    if pointee_inner != &scalar_inner {
                        return Err(LowerError::UnsupportedExpression(String::from(
                            "pointer-arg dynamic matrix cell writeback: pointee type mismatch",
                        )));
                    }
                    if srcs.len() != 1 {
                        return Err(LowerError::UnsupportedStatement(String::from(
                            "pointer-arg matrix element writeback expects one scalar",
                        )));
                    }
                    let lane_ty = naga_scalar_to_ir_type(scalar.kind)?;
                    let dsts = load_inout_vector_lanes(ctx, *arg_i, total, *scalar)?;
                    store_matrix_element_dynamic(
                        ctx, &dsts, col_v, index_v, nrows, srcs[0], lane_ty,
                    )?;
                    store_inout_vector_lanes(ctx, *arg_i, &dsts)
                }
                _ => Err(LowerError::UnsupportedExpression(String::from(
                    "pointer-arg Access writeback: unsupported base",
                ))),
            }
        }
        _ => Err(LowerError::UnsupportedExpression(String::from(
            "pointer-arg access writeback: unsupported expression shape",
        ))),
    }
}

fn vmctx_load_flat_by_offset_and_inner(
    ctx: &mut LowerCtx<'_>,
    byte_off: u32,
    value_inner: &TypeInner,
) -> Result<Vec<VReg>, LowerError> {
    let ir_tys = crate::lower_ctx::naga_type_to_ir_types(ctx.module, value_inner)?;
    let mut out = Vec::with_capacity(ir_tys.len());
    for (j, ty) in ir_tys.iter().enumerate() {
        let dst = ctx.fb.alloc_vreg(*ty);
        ctx.fb.push(LpirOp::Load {
            dst,
            base: VMCTX_VREG,
            offset: byte_off.saturating_add((j as u32).saturating_mul(4)),
        });
        out.push(dst);
    }
    Ok(out)
}

fn vmctx_store_flat_by_offset_and_inner(
    ctx: &mut LowerCtx<'_>,
    byte_off: u32,
    value_inner: &TypeInner,
    srcs: &[VReg],
) -> Result<(), LowerError> {
    let ir_tys = crate::lower_ctx::naga_type_to_ir_types(ctx.module, value_inner)?;
    if srcs.len() != ir_tys.len() {
        return Err(LowerError::Internal(format!(
            "global writeback vmctx store: {} vs {}",
            srcs.len(),
            ir_tys.len()
        )));
    }
    for (j, &s) in srcs.iter().enumerate() {
        ctx.fb.push(LpirOp::Store {
            base: VMCTX_VREG,
            offset: byte_off.saturating_add((j as u32).saturating_mul(4)),
            value: s,
        });
    }
    Ok(())
}

pub(crate) fn gv_root_value_inner_after_ptr(
    module: &crate::naga::Module,
    gv: Handle<GlobalVariable>,
) -> TypeInner {
    let mut ty = module.global_variables[gv].ty;
    while let TypeInner::Pointer { base: inner, .. } = module.types[ty].inner.clone() {
        ty = inner;
    }
    module.types[ty].inner.clone()
}

pub(crate) fn uniform_global_must_not_write(
    ctx: &LowerCtx<'_>,
    gv: Handle<GlobalVariable>,
) -> Result<(), LowerError> {
    if ctx.global_map.get(&gv).is_some_and(|g| g.is_uniform) {
        Err(LowerError::UnsupportedExpression(String::from(
            "cannot write to uniform variable",
        )))
    } else {
        Ok(())
    }
}

pub(crate) fn gv_vmctx_byte_offset(
    ctx: &LowerCtx<'_>,
    gv: Handle<GlobalVariable>,
) -> Result<u32, LowerError> {
    ctx.global_map
        .get(&gv)
        .map(|i| i.byte_offset)
        .ok_or_else(|| LowerError::Internal(String::from("global writeback: missing global_map")))
}

/// Private global scalar/vec/mat access leaves (matches [`store_vregs_through_local_access_leaf`]).
pub(crate) fn store_vregs_through_global_access_leaf(
    ctx: &mut LowerCtx<'_>,
    pointer: Handle<Expression>,
    pointee_inner: &TypeInner,
    srcs: &[VReg],
) -> Result<(), LowerError> {
    let expect = crate::lower_ctx::naga_type_to_ir_types(ctx.module, pointee_inner)?.len();
    if srcs.len() != expect {
        return Err(LowerError::Internal(format!(
            "global access writeback: expected {expect} vregs, got {}",
            srcs.len()
        )));
    }

    match &ctx.func.expressions[pointer] {
        Expression::Swizzle {
            size,
            vector,
            pattern,
        } => {
            let n = vector_size_usize(*size);
            if n != 1 {
                return Err(LowerError::UnsupportedExpression(String::from(
                    "global swizzle writeback: expected single component",
                )));
            }
            let Expression::GlobalVariable(gv_h) = &ctx.func.expressions[*vector] else {
                return Err(LowerError::UnsupportedExpression(String::from(
                    "global swizzle writeback: expected global vector root",
                )));
            };
            uniform_global_must_not_write(ctx, *gv_h)?;
            let comp = pattern[0] as usize;
            let root_inner = gv_root_value_inner_after_ptr(ctx.module, *gv_h);
            let TypeInner::Vector { .. } = &root_inner else {
                return Err(LowerError::UnsupportedExpression(String::from(
                    "global swizzle writeback: base not vector",
                )));
            };
            let off = gv_vmctx_byte_offset(ctx, *gv_h)?;
            let mut dsts = vmctx_load_flat_by_offset_and_inner(ctx, off, &root_inner)?;
            if comp >= dsts.len() {
                return Err(LowerError::UnsupportedStatement(format!(
                    "global swizzle: component {} OOB len {}",
                    comp,
                    dsts.len()
                )));
            }
            dsts[comp] = srcs[0];
            vmctx_store_flat_by_offset_and_inner(ctx, off, &root_inner, &dsts)
        }
        Expression::AccessIndex { base, index } => match &ctx.func.expressions[*base] {
            Expression::GlobalVariable(gv_h) => {
                uniform_global_must_not_write(ctx, *gv_h)?;
                let root_inner = gv_root_value_inner_after_ptr(ctx.module, *gv_h);
                let off = gv_vmctx_byte_offset(ctx, *gv_h)?;
                match &root_inner {
                    TypeInner::Vector { scalar, .. } => {
                        let comp = *index as usize;
                        let scalar_inner = TypeInner::Scalar(*scalar);
                        if pointee_inner != &scalar_inner || srcs.len() != 1 {
                            return Err(LowerError::UnsupportedStatement(String::from(
                                "global vector lane writeback: bad pointee",
                            )));
                        }
                        let mut dsts = vmctx_load_flat_by_offset_and_inner(ctx, off, &root_inner)?;
                        if comp >= dsts.len() {
                            return Err(LowerError::UnsupportedStatement(format!(
                                "global AccessIndex {comp} OOB len {}",
                                dsts.len()
                            )));
                        }
                        dsts[comp] = srcs[0];
                        vmctx_store_flat_by_offset_and_inner(ctx, off, &root_inner, &dsts)
                    }
                    TypeInner::Matrix {
                        columns,
                        rows,
                        scalar,
                    } => {
                        let ncols = vector_size_usize(*columns);
                        let nrows = vector_size_usize(*rows);
                        let col = *index as usize;
                        let col_ty = TypeInner::Vector {
                            size: *rows,
                            scalar: *scalar,
                        };
                        if pointee_inner != &col_ty || col >= ncols || srcs.len() != nrows {
                            return Err(LowerError::UnsupportedStatement(String::from(
                                "global matrix column writeback: bad dims or pointee",
                            )));
                        }
                        let mut dsts = vmctx_load_flat_by_offset_and_inner(ctx, off, &root_inner)?;
                        let start = col * nrows;
                        for r in 0..nrows {
                            dsts[start + r] = srcs[r];
                        }
                        vmctx_store_flat_by_offset_and_inner(ctx, off, &root_inner, &dsts)
                    }
                    _ => Err(LowerError::UnsupportedExpression(String::from(
                        "global AccessIndex writeback: bad matrix/vector root",
                    ))),
                }
            }
            Expression::AccessIndex {
                base: col_base,
                index: col_idx,
            } => {
                let Expression::GlobalVariable(gv_h) = &ctx.func.expressions[*col_base] else {
                    return Err(LowerError::UnsupportedStatement(String::from(
                        "nested global AccessIndex inner",
                    )));
                };
                uniform_global_must_not_write(ctx, *gv_h)?;
                let root_inner = gv_root_value_inner_after_ptr(ctx.module, *gv_h);
                let TypeInner::Matrix {
                    columns,
                    rows,
                    scalar,
                } = root_inner
                else {
                    return Err(LowerError::UnsupportedStatement(String::from(
                        "nested global AccessIndex matrix cell: not matrix",
                    )));
                };
                let nrows = vector_size_usize(rows);
                let ncols = vector_size_usize(columns);
                let col = *col_idx as usize;
                let row = *index as usize;
                if col >= ncols || row >= nrows {
                    return Err(LowerError::UnsupportedStatement(String::from(
                        "matrix indices OOB",
                    )));
                }
                let flat_i = col * nrows + row;
                let scalar_inner = TypeInner::Scalar(scalar);
                if pointee_inner != &scalar_inner || srcs.len() != 1 {
                    return Err(LowerError::UnsupportedStatement(String::from(
                        "matrix cell mismatch",
                    )));
                }
                let off = gv_vmctx_byte_offset(ctx, *gv_h)?;
                let mut dsts = vmctx_load_flat_by_offset_and_inner(ctx, off, &root_inner)?;
                if flat_i >= dsts.len() {
                    return Err(LowerError::UnsupportedStatement(format!(
                        "flat {flat_i} OOB",
                    )));
                }
                dsts[flat_i] = srcs[0];
                vmctx_store_flat_by_offset_and_inner(ctx, off, &root_inner, &dsts)
            }
            _ => Err(LowerError::UnsupportedExpression(String::from(
                "global AccessIndex writeback: unsupported inner",
            ))),
        },
        Expression::Access { base, index } => {
            let index_v = ctx.ensure_expr(*index)?;
            match &ctx.func.expressions[*base] {
                Expression::GlobalVariable(gv_h) => {
                    uniform_global_must_not_write(ctx, *gv_h)?;
                    let root_inner = gv_root_value_inner_after_ptr(ctx.module, *gv_h);
                    let off = gv_vmctx_byte_offset(ctx, *gv_h)?;
                    match &root_inner {
                        TypeInner::Vector { scalar, .. } => {
                            let lane_ty = naga_scalar_to_ir_type(scalar.kind)?;
                            if pointee_inner != &TypeInner::Scalar(*scalar) || srcs.len() != 1 {
                                return Err(LowerError::UnsupportedStatement(String::from(
                                    "dyn global vector lane mismatch",
                                )));
                            }
                            let dsts = vmctx_load_flat_by_offset_and_inner(ctx, off, &root_inner)?;
                            merge_vector_lane_store(ctx, &dsts, index_v, srcs[0], lane_ty)?;
                            vmctx_store_flat_by_offset_and_inner(ctx, off, &root_inner, &dsts)
                        }
                        TypeInner::Matrix {
                            rows,
                            scalar,
                            columns: _,
                        } => {
                            let lane_ty = naga_scalar_to_ir_type(scalar.kind)?;
                            let nrows = vector_size_usize(*rows);
                            let col_ty = TypeInner::Vector {
                                size: *rows,
                                scalar: *scalar,
                            };
                            if pointee_inner != &col_ty || srcs.len() != nrows {
                                return Err(LowerError::UnsupportedStatement(String::from(
                                    "dyn global matrix column mismatch",
                                )));
                            }
                            let dsts_vec =
                                vmctx_load_flat_by_offset_and_inner(ctx, off, &root_inner)?;
                            store_matrix_column_dynamic(
                                ctx, &dsts_vec, index_v, nrows, srcs, lane_ty,
                            )?;
                            vmctx_store_flat_by_offset_and_inner(ctx, off, &root_inner, &dsts_vec)
                        }
                        _ => Err(LowerError::UnsupportedExpression(String::from(
                            "dynamic global Access unsupported root ty",
                        ))),
                    }
                }
                Expression::Access {
                    base: mat_base,
                    index: col_idx_h,
                } => {
                    let Expression::GlobalVariable(gv_h) = &ctx.func.expressions[*mat_base] else {
                        return Err(LowerError::UnsupportedStatement(String::from(
                            "nested global Access matrix inner",
                        )));
                    };
                    uniform_global_must_not_write(ctx, *gv_h)?;
                    let ri = gv_root_value_inner_after_ptr(ctx.module, *gv_h);
                    let col_v = ctx.ensure_expr(*col_idx_h)?;
                    let (nrows, lane_ty, sk) = match &ri {
                        TypeInner::Matrix { rows, scalar, .. } => (
                            vector_size_usize(*rows),
                            naga_scalar_to_ir_type(scalar.kind)?,
                            *scalar,
                        ),
                        _ => {
                            return Err(LowerError::UnsupportedStatement(String::from(
                                "nested global Access not matrix value",
                            )));
                        }
                    };
                    if pointee_inner != &TypeInner::Scalar(sk) || srcs.len() != 1 {
                        return Err(LowerError::UnsupportedStatement(String::from(
                            "nested matrix cell mismatch",
                        )));
                    }
                    let off = gv_vmctx_byte_offset(ctx, *gv_h)?;
                    let dsts_vec = vmctx_load_flat_by_offset_and_inner(ctx, off, &ri)?;
                    store_matrix_element_dynamic(
                        ctx, &dsts_vec, col_v, index_v, nrows, srcs[0], lane_ty,
                    )?;
                    vmctx_store_flat_by_offset_and_inner(ctx, off, &ri, &dsts_vec)
                }
                _ => Err(LowerError::UnsupportedExpression(String::from(
                    "dynamic global Access unsupported nested base",
                ))),
            }
        }
        _ => Err(LowerError::UnsupportedExpression(String::from(
            "global access writeback: unsupported expression shape",
        ))),
    }
}
