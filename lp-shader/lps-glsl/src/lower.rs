use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use lp_collection::VecMap;

use lpir::{
    CalleeRef, FuncId, FunctionBuilder, ImportDecl, IrType, LpirModule, LpirOp, ModuleBuilder,
    VMCTX_VREG, VReg,
};
use lps_shared::{LpsModuleSig, LpsType, ParamQualifier, TextureBindingSpec};

use crate::body::{BinaryOp, UnaryOp};
use crate::hir::{
    ExprId, ExprList, HirArena, HirExprKind, HirFunction, HirModule, HirStmt, ImportKey, PlaceId,
    scalar_base_type, scalar_ir_types, scalar_lane_count,
};
use crate::{Diagnostic, Span};

mod ops;
mod place;
mod storage;

use ops::{
    assign_target, lower_binary, lower_builtin, lower_cast, lower_inc_dec, lower_index,
    lower_select, lower_texel_fetch, lower_texture_sample, read_assign_target, single_lane,
};
use storage::{
    LocalStorage, alloc_slot_addr, flat_value_byte_size, is_pointer_param, load_value_from_addr,
    local_storage, local_value, lower_global_load, lower_uniform_load, param_pointer, store_local,
    store_value_to_addr,
};

#[derive(Debug, Clone)]
pub struct LoweredModule {
    pub ir: LpirModule,
    pub meta: LpsModuleSig,
}

pub fn lower_hir(module: HirModule) -> Result<LoweredModule, Diagnostic> {
    let mut mb = ModuleBuilder::new();
    let mut import_map = VecMap::new();
    for import in &module.imports {
        let callee = mb.add_import(ImportDecl {
            module_name: import.module_name.clone(),
            func_name: import.func_name.clone(),
            param_types: import.param_types.clone(),
            return_types: import.return_types.clone(),
            lpfn_glsl_params: import.lpfn_glsl_params.clone(),
            needs_vmctx: matches!(import.key, ImportKey::Vm { .. }),
            sret: import.sret,
        });
        import_map.insert(import.key.clone(), callee);
    }

    for function in &module.functions {
        let lowered = lower_function(function, &module, &import_map)?;
        mb.add_function(lowered);
    }
    let ir = mb.finish();
    // Validation is a development invariant check on the lowering itself, not
    // part of the product compile: release firmware skips it (decision
    // 2026-07-05, compile-churn-p0 plan — it cost a 3-4x re-walk of the op
    // stream on device). Host dev builds, unit tests, and dev-profile
    // filetest runs still validate every compile.
    #[cfg(debug_assertions)]
    if let Err(errors) = lpir::validate_module(&ir) {
        let message = errors.first().map_or_else(
            || String::from("unknown LPIR validation error"),
            alloc::string::ToString::to_string,
        );
        return Err(Diagnostic::error(
            Span::new(0, 0),
            format!("generated LPIR failed validation: {message}"),
        ));
    }
    Ok(LoweredModule {
        ir,
        meta: module.meta,
    })
}

fn lower_function(
    function: &HirFunction,
    module: &HirModule,
    import_map: &VecMap<ImportKey, CalleeRef>,
) -> Result<lpir::IrFunction, Diagnostic> {
    let return_types = scalar_ir_types(&function.return_ty)?;
    let mut fb = FunctionBuilder::new(&function.name, &return_types);
    let mut params = Vec::new();
    for param in &function.params {
        let lanes = if matches!(param.qualifier, ParamQualifier::Out | ParamQualifier::InOut) {
            vec![fb.add_param(IrType::Pointer)]
        } else {
            scalar_ir_types(&param.ty)?
                .into_iter()
                .map(|ty| fb.add_param(ty))
                .collect()
        };
        params.push(LowerValue {
            ty: param.ty.clone(),
            lanes,
        });
    }
    let vmctx = fb.alloc_vreg(IrType::Pointer);
    fb.push(LpirOp::Copy {
        dst: vmctx,
        src: VMCTX_VREG,
    });
    let mut locals = Vec::new();
    for local in &function.body.locals {
        locals.push(local_storage(&mut fb, local.ty.clone())?);
    }
    let mut ctx = LowerCtx {
        fb,
        vmctx,
        params,
        locals,
        arena: &function.body.arena,
        import_map,
        param_qualifiers: function.params.iter().map(|p| p.qualifier).collect(),
        texture_specs: &module.texture_specs,
        texel_fetch_bounds: module.texel_fetch_bounds,
    };
    lower_statements(&mut ctx, &function.body.statements)?;
    if function.return_ty == LpsType::Void {
        ctx.fb.push_return(&[]);
    }
    Ok(ctx.fb.finish())
}

struct LowerCtx<'a> {
    fb: FunctionBuilder,
    vmctx: VReg,
    params: Vec<LowerValue>,
    locals: Vec<LocalStorage>,
    arena: &'a HirArena,
    import_map: &'a VecMap<ImportKey, CalleeRef>,
    param_qualifiers: Vec<ParamQualifier>,
    texture_specs: &'a VecMap<String, TextureBindingSpec>,
    texel_fetch_bounds: lpir::TexelFetchBoundsMode,
}

#[derive(Debug, Clone)]
struct LowerValue {
    ty: LpsType,
    lanes: Vec<VReg>,
}

fn lower_statements(ctx: &mut LowerCtx<'_>, statements: &[HirStmt]) -> Result<(), Diagnostic> {
    for stmt in statements {
        lower_stmt(ctx, stmt)?;
        if stmt_terminates_block(stmt) {
            break;
        }
    }
    Ok(())
}

fn stmt_terminates_block(stmt: &HirStmt) -> bool {
    match stmt {
        HirStmt::Return { .. } | HirStmt::Break | HirStmt::Continue => true,
        HirStmt::If { accept, reject, .. } => {
            !reject.is_empty()
                && statements_terminate_block(accept)
                && statements_terminate_block(reject)
        }
        _ => false,
    }
}

fn statements_terminate_block(statements: &[HirStmt]) -> bool {
    statements.iter().any(stmt_terminates_block)
}

fn lower_stmt(ctx: &mut LowerCtx<'_>, stmt: &HirStmt) -> Result<(), Diagnostic> {
    match stmt {
        HirStmt::Let { local, init } => {
            let span = ctx.arena.expr_span(*init);
            let value = lower_expr(ctx, *init)?;
            store_local(ctx, span, *local, &value)
        }
        HirStmt::Assign { local, value } => {
            let span = ctx.arena.expr_span(*value);
            let value = lower_expr(ctx, *value)?;
            store_local(ctx, span, *local, &value)
        }
        HirStmt::If {
            condition,
            accept,
            reject,
        } => {
            let cond = lower_expr(ctx, *condition)?;
            let cond = single_lane(ctx.arena.expr_span(*condition), &cond)?;
            ctx.fb.push_if(cond);
            lower_statements(ctx, accept)?;
            if !reject.is_empty() {
                ctx.fb.push_else();
                lower_statements(ctx, reject)?;
            }
            ctx.fb.end_if();
            Ok(())
        }
        HirStmt::For {
            init,
            condition,
            continuing,
            body,
        } => {
            lower_statements(ctx, init)?;
            ctx.fb.push_loop();
            let cond = lower_expr(ctx, *condition)?;
            let cond = single_lane(ctx.arena.expr_span(*condition), &cond)?;
            ctx.fb.push(LpirOp::BrIfNot { cond });
            lower_statements(ctx, body)?;
            ctx.fb.push_continuing();
            lower_statements(ctx, continuing)?;
            ctx.fb.end_loop();
            Ok(())
        }
        HirStmt::While { condition, body } => {
            ctx.fb.push_loop();
            let cond = lower_expr(ctx, *condition)?;
            let cond = single_lane(ctx.arena.expr_span(*condition), &cond)?;
            ctx.fb.push(LpirOp::BrIfNot { cond });
            lower_statements(ctx, body)?;
            ctx.fb.push_continuing();
            ctx.fb.end_loop();
            Ok(())
        }
        HirStmt::DoWhile { body, condition } => {
            ctx.fb.push_loop();
            lower_statements(ctx, body)?;
            ctx.fb.push_continuing();
            let cond = lower_expr(ctx, *condition)?;
            let cond = single_lane(ctx.arena.expr_span(*condition), &cond)?;
            ctx.fb.push(LpirOp::BrIfNot { cond });
            ctx.fb.end_loop();
            Ok(())
        }
        HirStmt::Break => {
            ctx.fb.push(LpirOp::Break);
            Ok(())
        }
        HirStmt::Continue => {
            ctx.fb.push(LpirOp::Continue);
            Ok(())
        }
        HirStmt::Expr(expr) => {
            let _ = lower_expr(ctx, *expr)?;
            Ok(())
        }
        HirStmt::Return { expr, span } => {
            let lanes = return_lanes(ctx, *span, *expr)?;
            ctx.fb.push_return(&lanes);
            Ok(())
        }
    }
}

fn return_lanes(
    ctx: &mut LowerCtx<'_>,
    span: Span,
    expr: Option<ExprId>,
) -> Result<Vec<VReg>, Diagnostic> {
    let lanes = match expr {
        Some(expr) => lower_expr(ctx, expr)?.lanes,
        None => Vec::new(),
    };
    if lanes.is_empty() && expr.is_some() {
        return Err(Diagnostic::error(span, "return expression has no value"));
    }
    Ok(lanes)
}

fn lower_expr(ctx: &mut LowerCtx<'_>, expr: ExprId) -> Result<LowerValue, Diagnostic> {
    let expr = ctx.arena.expr(expr);
    match &expr.kind {
        HirExprKind::BoolLiteral(v) => {
            let dst = ctx.fb.alloc_vreg(IrType::I32);
            ctx.fb.push(LpirOp::IconstI32 {
                dst,
                value: if *v { 1 } else { 0 },
            });
            Ok(LowerValue {
                ty: LpsType::Bool,
                lanes: vec![dst],
            })
        }
        HirExprKind::FloatLiteral(v) => {
            let dst = ctx.fb.alloc_vreg(IrType::F32);
            ctx.fb.push(LpirOp::FconstF32 { dst, value: *v });
            Ok(LowerValue {
                ty: LpsType::Float,
                lanes: vec![dst],
            })
        }
        HirExprKind::IntLiteral(v) => {
            let dst = ctx.fb.alloc_vreg(IrType::I32);
            ctx.fb.push(LpirOp::IconstI32 { dst, value: *v });
            Ok(LowerValue {
                ty: LpsType::Int,
                lanes: vec![dst],
            })
        }
        HirExprKind::UIntLiteral(v) => {
            let dst = ctx.fb.alloc_vreg(IrType::I32);
            ctx.fb.push(LpirOp::IconstI32 {
                dst,
                value: i32::from_ne_bytes(v.to_ne_bytes()),
            });
            Ok(LowerValue {
                ty: LpsType::UInt,
                lanes: vec![dst],
            })
        }
        HirExprKind::Param { index } => {
            if is_pointer_param(ctx, *index) {
                let addr = param_pointer(ctx, expr.span, *index)?;
                load_value_from_addr(ctx, expr.span, addr, &expr.ty)
            } else {
                ctx.params.get(*index).cloned().ok_or_else(|| {
                    Diagnostic::error(
                        expr.span,
                        format!("parameter index {index} is out of range"),
                    )
                })
            }
        }
        HirExprKind::Local { index } => local_value(ctx, expr.span, *index),
        HirExprKind::Uniform { byte_offset } => {
            lower_uniform_load(ctx, expr.span, *byte_offset, &expr.ty)
        }
        HirExprKind::Global { byte_offset } => {
            lower_global_load(ctx, expr.span, *byte_offset, &expr.ty)
        }
        HirExprKind::Constructor { args } => {
            let mut lanes = Vec::new();
            let args = ctx.arena.expr_list(*args).to_vec();
            if expr.ty.is_matrix() && args.len() == 1 && ctx.arena.expr_ty(args[0]).is_matrix() {
                let value = lower_expr(ctx, args[0])?;
                let Some((dst_cols, dst_rows)) = expr.ty.matrix_dims() else {
                    return Err(Diagnostic::error(expr.span, "invalid matrix constructor"));
                };
                let Some((src_cols, src_rows)) = ctx.arena.expr_ty(args[0]).matrix_dims() else {
                    return Err(Diagnostic::error(
                        ctx.arena.expr_span(args[0]),
                        "invalid source matrix",
                    ));
                };
                for col in 0..dst_cols {
                    for row in 0..dst_rows {
                        if col < src_cols && row < src_rows {
                            lanes.push(value.lanes[col * src_rows + row]);
                        } else if col == row {
                            let one = ctx.fb.alloc_vreg(IrType::F32);
                            ctx.fb.push(LpirOp::FconstF32 {
                                dst: one,
                                value: 1.0,
                            });
                            lanes.push(one);
                        } else {
                            let zero = ctx.fb.alloc_vreg(IrType::F32);
                            ctx.fb.push(LpirOp::FconstF32 {
                                dst: zero,
                                value: 0.0,
                            });
                            lanes.push(zero);
                        }
                    }
                }
            } else if expr.ty.is_matrix()
                && args.len() == 1
                && scalar_lane_count(ctx.arena.expr_ty(args[0])) == 1
            {
                let value = lower_expr(ctx, args[0])?;
                let diagonal = single_lane(ctx.arena.expr_span(args[0]), &value)?;
                let Some((cols, rows)) = expr.ty.matrix_dims() else {
                    return Err(Diagnostic::error(expr.span, "invalid matrix constructor"));
                };
                for col in 0..cols {
                    for row in 0..rows {
                        if col == row {
                            lanes.push(diagonal);
                        } else {
                            let zero = ctx.fb.alloc_vreg(IrType::F32);
                            ctx.fb.push(LpirOp::FconstF32 {
                                dst: zero,
                                value: 0.0,
                            });
                            lanes.push(zero);
                        }
                    }
                }
            } else if args.len() == 1
                && scalar_lane_count(&expr.ty) > 1
                && scalar_lane_count(ctx.arena.expr_ty(args[0])) == 1
            {
                let value = lower_expr(ctx, args[0])?;
                let lane = single_lane(ctx.arena.expr_span(args[0]), &value)?;
                lanes.resize(scalar_lane_count(&expr.ty), lane);
            } else {
                for arg in args {
                    lanes.extend(lower_expr(ctx, arg)?.lanes);
                }
                lanes.truncate(scalar_lane_count(&expr.ty));
            }
            Ok(LowerValue {
                ty: expr.ty.clone(),
                lanes,
            })
        }
        HirExprKind::Cast { expr: inner } => {
            let inner = lower_expr(ctx, *inner)?;
            lower_cast(ctx, expr.span, inner, &expr.ty)
        }
        HirExprKind::Swizzle { base, lanes } => {
            let base = lower_expr(ctx, *base)?;
            let out = lanes.iter().map(|i| base.lanes[*i]).collect::<Vec<_>>();
            Ok(LowerValue {
                ty: expr.ty.clone(),
                lanes: out,
            })
        }
        HirExprKind::Index { base, index } => {
            let base = lower_expr(ctx, *base)?;
            let index = lower_expr(ctx, *index)?;
            lower_index(ctx, expr.span, base, index, &expr.ty)
        }
        HirExprKind::Builtin {
            kind,
            args,
            writebacks,
        } => lower_builtin(
            ctx,
            expr.span,
            *kind,
            ctx.arena.expr_list(*args),
            writebacks,
            &expr.ty,
        ),
        HirExprKind::UserCall {
            function,
            args,
            writebacks,
        } => {
            let mut writeback_slots = Vec::new();
            let mut arg_lanes = vec![ctx.vmctx];
            for (arg_index, arg) in ctx.arena.expr_list(*args).iter().copied().enumerate() {
                if let Some(writeback) = writebacks.iter().find(|w| w.arg_index == arg_index) {
                    let (_slot, addr) =
                        alloc_slot_addr(ctx, flat_value_byte_size(&writeback.ty), IrType::Pointer);
                    if writeback.copy_in {
                        let value = lower_expr(ctx, arg)?;
                        store_value_to_addr(ctx, expr.span, addr, &value)?;
                    }
                    arg_lanes.push(addr);
                    writeback_slots.push((writeback, addr));
                } else {
                    arg_lanes.extend(lower_expr(ctx, arg)?.lanes);
                }
            }
            let results = scalar_ir_types(&expr.ty)?
                .into_iter()
                .map(|ty| ctx.fb.alloc_vreg(ty))
                .collect::<Vec<_>>();
            ctx.fb.push_call(
                CalleeRef::Local(FuncId(*function as u16)),
                &arg_lanes,
                &results,
            );
            for (writeback, addr) in writeback_slots {
                let value = load_value_from_addr(ctx, expr.span, addr, &writeback.ty)?;
                assign_target(ctx, expr.span, writeback.target, value)?;
            }
            Ok(LowerValue {
                ty: expr.ty.clone(),
                lanes: results,
            })
        }
        HirExprKind::ImportCall { import, args, out } => {
            let callee = *ctx.import_map.get(import).ok_or_else(|| {
                Diagnostic::error(expr.span, format!("missing import for {import:?}"))
            })?;
            if let Some(out) = out {
                return lower_import_call_with_out(ctx, expr.span, callee, args, out, &expr.ty);
            }
            if matches!(import, ImportKey::Glsl { .. }) && scalar_lane_count(&expr.ty) > 1 {
                let args = ctx
                    .arena
                    .expr_list(*args)
                    .iter()
                    .map(|arg| lower_expr(ctx, *arg))
                    .collect::<Result<Vec<_>, _>>()?;
                let mut results = Vec::new();
                for i in 0..scalar_lane_count(&expr.ty) {
                    let call_args = args
                        .iter()
                        .map(|arg| ops::lane_at(arg, i))
                        .collect::<Vec<_>>();
                    let dst = ctx.fb.alloc_vreg(IrType::F32);
                    ctx.fb.push_call(callee, &call_args, &[dst]);
                    results.push(dst);
                }
                return Ok(LowerValue {
                    ty: expr.ty.clone(),
                    lanes: results,
                });
            }
            let mut arg_lanes = Vec::new();
            if matches!(import, ImportKey::Vm { .. }) {
                arg_lanes.push(ctx.vmctx);
            }
            for arg in ctx.arena.expr_list(*args).iter().copied() {
                arg_lanes.extend(lower_expr(ctx, arg)?.lanes);
            }
            let results = scalar_ir_types(&expr.ty)?
                .into_iter()
                .map(|ty| ctx.fb.alloc_vreg(ty))
                .collect::<Vec<_>>();
            ctx.fb.push_call(callee, &arg_lanes, &results);
            Ok(LowerValue {
                ty: expr.ty.clone(),
                lanes: results,
            })
        }
        HirExprKind::TexelFetch {
            sampler,
            coord,
            lod,
        } => lower_texel_fetch(ctx, expr.span, sampler, *coord, *lod),
        HirExprKind::Texture {
            sampler,
            coord,
            import,
        } => lower_texture_sample(ctx, expr.span, sampler, *coord, import),
        HirExprKind::Unary { op, expr: inner } => {
            let inner = lower_expr(ctx, *inner)?;
            match (op, inner.ty.clone()) {
                (UnaryOp::Neg, ty) if scalar_base_type(&ty) == Some(LpsType::Float) => {
                    let lanes = inner
                        .lanes
                        .iter()
                        .map(|src| {
                            let dst = ctx.fb.alloc_vreg(IrType::F32);
                            ctx.fb.push(LpirOp::Fneg { dst, src: *src });
                            dst
                        })
                        .collect::<Vec<_>>();
                    Ok(LowerValue { ty, lanes })
                }
                (UnaryOp::Neg, ty) if scalar_base_type(&ty) == Some(LpsType::Int) => {
                    let lanes = inner
                        .lanes
                        .iter()
                        .map(|src| {
                            let dst = ctx.fb.alloc_vreg(IrType::I32);
                            ctx.fb.push(LpirOp::Ineg { dst, src: *src });
                            dst
                        })
                        .collect::<Vec<_>>();
                    Ok(LowerValue { ty, lanes })
                }
                (UnaryOp::Not, LpsType::Bool) => {
                    let src = single_lane(expr.span, &inner)?;
                    let one = ctx.fb.alloc_vreg(IrType::I32);
                    ctx.fb.push(LpirOp::IconstI32 { dst: one, value: 1 });
                    let dst = ctx.fb.alloc_vreg(IrType::I32);
                    ctx.fb.push(LpirOp::Ixor {
                        dst,
                        lhs: src,
                        rhs: one,
                    });
                    Ok(LowerValue {
                        ty: LpsType::Bool,
                        lanes: vec![dst],
                    })
                }
                _ => Err(Diagnostic::error(expr.span, "unsupported unary lowering")),
            }
        }
        HirExprKind::Binary { op, lhs, rhs } => {
            if matches!(op, BinaryOp::LogicalAnd | BinaryOp::LogicalOr)
                && expr_needs_lazy_eval(ctx.arena, *rhs)
            {
                lower_short_circuit(ctx, *op, *lhs, *rhs)
            } else {
                let lhs = lower_expr(ctx, *lhs)?;
                let rhs = lower_expr(ctx, *rhs)?;
                lower_binary(ctx, expr.span, *op, lhs, rhs, &expr.ty)
            }
        }
        HirExprKind::Sequence { first, second } => {
            let _ = lower_expr(ctx, *first)?;
            lower_expr(ctx, *second)
        }
        HirExprKind::Conditional {
            condition,
            accept,
            reject,
        } => {
            if expr_needs_lazy_eval(ctx.arena, *accept) || expr_needs_lazy_eval(ctx.arena, *reject)
            {
                lower_lazy_conditional(ctx, expr.span, *condition, *accept, *reject, &expr.ty)
            } else {
                let condition = lower_expr(ctx, *condition)?;
                let accept = lower_expr(ctx, *accept)?;
                let reject = lower_expr(ctx, *reject)?;
                lower_select(ctx, expr.span, condition, accept, reject, &expr.ty)
            }
        }
        HirExprKind::PlaceRead { target } => read_assign_target(ctx, expr.span, *target),
        HirExprKind::Assign { target, value } => {
            let value = lower_expr(ctx, *value)?;
            assign_target(ctx, expr.span, *target, value.clone())?;
            Ok(value)
        }
        HirExprKind::IncDec { target, op, prefix } => {
            lower_inc_dec(ctx, expr.span, *target, *op, *prefix)
        }
    }
}

/// True when an expression must not be evaluated speculatively: it has side
/// effects (calls that may write state, assignments, increments) or can trap
/// on some backend (integer division / modulo is unguarded `i32.div_s` on
/// wasm). GLSL `&&` / `||` / `?:` only evaluate the operands the spec says
/// they evaluate; operands for which this returns false are safe to keep on
/// the branchless eager path (`iand` / `ior` / `select`).
fn expr_needs_lazy_eval(arena: &HirArena, expr: ExprId) -> bool {
    let e = arena.expr(expr);
    match &e.kind {
        HirExprKind::BoolLiteral(_)
        | HirExprKind::FloatLiteral(_)
        | HirExprKind::IntLiteral(_)
        | HirExprKind::UIntLiteral(_)
        | HirExprKind::Param { .. }
        | HirExprKind::Local { .. }
        | HirExprKind::Uniform { .. }
        | HirExprKind::Global { .. } => false,
        HirExprKind::Constructor { args } => expr_list_needs_lazy_eval(arena, *args),
        HirExprKind::Cast { expr } | HirExprKind::Swizzle { base: expr, .. } => {
            expr_needs_lazy_eval(arena, *expr)
        }
        HirExprKind::Index { base, index } => {
            expr_needs_lazy_eval(arena, *base) || expr_needs_lazy_eval(arena, *index)
        }
        HirExprKind::Builtin {
            args, writebacks, ..
        } => !writebacks.is_empty() || expr_list_needs_lazy_eval(arena, *args),
        // User functions may write globals; treat every call as effectful.
        HirExprKind::UserCall { .. } => true,
        HirExprKind::ImportCall { args, out, .. } => {
            out.is_some() || expr_list_needs_lazy_eval(arena, *args)
        }
        HirExprKind::TexelFetch { coord, lod, .. } => {
            expr_needs_lazy_eval(arena, *coord) || expr_needs_lazy_eval(arena, *lod)
        }
        HirExprKind::Texture { coord, .. } => expr_needs_lazy_eval(arena, *coord),
        HirExprKind::Unary { expr, .. } => expr_needs_lazy_eval(arena, *expr),
        HirExprKind::Binary { op, lhs, rhs } => {
            let int_div_may_trap = matches!(op, BinaryOp::Div | BinaryOp::Mod)
                && matches!(
                    scalar_base_type(&e.ty).as_ref().unwrap_or(&e.ty),
                    LpsType::Int | LpsType::UInt
                );
            int_div_may_trap
                || expr_needs_lazy_eval(arena, *lhs)
                || expr_needs_lazy_eval(arena, *rhs)
        }
        HirExprKind::Sequence { first, second } => {
            expr_needs_lazy_eval(arena, *first) || expr_needs_lazy_eval(arena, *second)
        }
        HirExprKind::Conditional {
            condition,
            accept,
            reject,
        } => {
            expr_needs_lazy_eval(arena, *condition)
                || expr_needs_lazy_eval(arena, *accept)
                || expr_needs_lazy_eval(arena, *reject)
        }
        HirExprKind::PlaceRead { target } => place_needs_lazy_eval(arena, *target),
        HirExprKind::Assign { .. } | HirExprKind::IncDec { .. } => true,
    }
}

fn expr_list_needs_lazy_eval(arena: &HirArena, list: ExprList) -> bool {
    arena
        .expr_list(list)
        .iter()
        .any(|arg| expr_needs_lazy_eval(arena, *arg))
}

fn place_needs_lazy_eval(arena: &HirArena, place: PlaceId) -> bool {
    arena.place(place).segments.iter().any(|seg| match seg {
        crate::hir::PlaceSegment::Index { index, .. } => expr_needs_lazy_eval(arena, *index),
        _ => false,
    })
}

/// GLSL `&&` / `||` must short-circuit: the right operand is evaluated only
/// when the left operand does not already decide the result. Lower to an
/// if/else where one arm evaluates the right operand and the other reuses the
/// left value. The result vreg is defined in both arms (never before the
/// `if`) — the native backend's allocator supports conditional defs only in
/// this both-arms shape, the same one naga-frontend local stores produce.
fn lower_short_circuit(
    ctx: &mut LowerCtx<'_>,
    op: BinaryOp,
    lhs: ExprId,
    rhs: ExprId,
) -> Result<LowerValue, Diagnostic> {
    let lhs_span = ctx.arena.expr_span(lhs);
    let lhs = lower_expr(ctx, lhs)?;
    let lhs_lane = single_lane(lhs_span, &lhs)?;
    let result = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push_if(lhs_lane);
    if op == BinaryOp::LogicalOr {
        ctx.fb.push(LpirOp::Copy {
            dst: result,
            src: lhs_lane,
        });
        ctx.fb.push_else();
    }
    let rhs_span = ctx.arena.expr_span(rhs);
    let rhs = lower_expr(ctx, rhs)?;
    let rhs_lane = single_lane(rhs_span, &rhs)?;
    ctx.fb.push(LpirOp::Copy {
        dst: result,
        src: rhs_lane,
    });
    if op == BinaryOp::LogicalAnd {
        ctx.fb.push_else();
        ctx.fb.push(LpirOp::Copy {
            dst: result,
            src: lhs_lane,
        });
    }
    ctx.fb.end_if();
    Ok(LowerValue {
        ty: LpsType::Bool,
        lanes: vec![result],
    })
}

/// GLSL `?:` evaluates only the selected operand. Used when an arm needs lazy
/// evaluation; pure arms keep the branchless `select` path. Same both-arms
/// result-def shape as [`lower_short_circuit`].
fn lower_lazy_conditional(
    ctx: &mut LowerCtx<'_>,
    span: Span,
    condition: ExprId,
    accept: ExprId,
    reject: ExprId,
    result_ty: &LpsType,
) -> Result<LowerValue, Diagnostic> {
    let cond = lower_expr(ctx, condition)?;
    let cond_lane = single_lane(ctx.arena.expr_span(condition), &cond)?;
    let lanes = scalar_ir_types(result_ty)?
        .into_iter()
        .map(|ty| ctx.fb.alloc_vreg(ty))
        .collect::<Vec<_>>();
    ctx.fb.push_if(cond_lane);
    lower_conditional_arm(ctx, span, accept, &lanes)?;
    ctx.fb.push_else();
    lower_conditional_arm(ctx, span, reject, &lanes)?;
    ctx.fb.end_if();
    Ok(LowerValue {
        ty: result_ty.clone(),
        lanes,
    })
}

fn lower_conditional_arm(
    ctx: &mut LowerCtx<'_>,
    span: Span,
    arm: ExprId,
    result_lanes: &[VReg],
) -> Result<(), Diagnostic> {
    let value = lower_expr(ctx, arm)?;
    if value.lanes.len() != result_lanes.len() {
        return Err(Diagnostic::error(span, "conditional arm lane mismatch"));
    }
    for (dst, src) in result_lanes.iter().zip(value.lanes.iter()) {
        ctx.fb.push(LpirOp::Copy {
            dst: *dst,
            src: *src,
        });
    }
    Ok(())
}

fn lower_import_call_with_out(
    ctx: &mut LowerCtx<'_>,
    span: Span,
    callee: CalleeRef,
    args: &ExprList,
    out: &crate::hir::HirOutArg,
    result_ty: &LpsType,
) -> Result<LowerValue, Diagnostic> {
    let out_lanes = scalar_lane_count(&out.ty);
    let (_slot, addr) = alloc_slot_addr(ctx, out_lanes as u32 * 4, IrType::I32);

    let mut arg_lanes = Vec::new();
    let mut value_arg = 0usize;
    let args = ctx.arena.expr_list(*args);
    for arg_index in 0..(args.len() + 1) {
        if arg_index == out.arg_index {
            arg_lanes.push(addr);
        } else {
            let arg = args.get(value_arg).copied().ok_or_else(|| {
                Diagnostic::error(span, "internal lpfn out argument lowering mismatch")
            })?;
            arg_lanes.extend(lower_expr(ctx, arg)?.lanes);
            value_arg += 1;
        }
    }

    let results = scalar_ir_types(result_ty)?
        .into_iter()
        .map(|ty| ctx.fb.alloc_vreg(ty))
        .collect::<Vec<_>>();
    ctx.fb.push_call(callee, &arg_lanes, &results);

    let mut lanes = Vec::new();
    for i in 0..out_lanes {
        let tmp = ctx.fb.alloc_vreg(IrType::F32);
        ctx.fb.push(LpirOp::Load {
            dst: tmp,
            base: addr,
            offset: i as u32 * 4,
        });
        lanes.push(tmp);
    }
    let out_value = LowerValue {
        ty: out.ty.clone(),
        lanes,
    };
    store_local(ctx, span, out.local, &out_value)?;

    Ok(LowerValue {
        ty: result_ty.clone(),
        lanes: results,
    })
}
