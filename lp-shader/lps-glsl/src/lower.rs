use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use lpir::{
    CalleeRef, FuncId, FunctionBuilder, ImportDecl, IrType, LpirModule, LpirOp, ModuleBuilder,
    VMCTX_VREG, VReg,
};
use lps_shared::{LpsModuleSig, LpsType};

use crate::body::{BinaryOp, IncDecOp, UnaryOp};
use crate::hir::{
    BuiltinKind, HirExpr, HirExprKind, HirFunction, HirModule, HirStmt, ImportKey,
    scalar_base_type, scalar_ir_types, scalar_lane_count,
};
use crate::{Diagnostic, Span};

#[derive(Debug, Clone)]
pub struct LoweredModule {
    pub ir: LpirModule,
    pub meta: LpsModuleSig,
}

pub fn lower_hir(module: HirModule) -> Result<LoweredModule, Diagnostic> {
    let mut mb = ModuleBuilder::new();
    let mut import_map = BTreeMap::new();
    for import in &module.imports {
        let callee = mb.add_import(ImportDecl {
            module_name: import.module_name.clone(),
            func_name: import.func_name.clone(),
            param_types: import.param_types.clone(),
            return_types: import.return_types.clone(),
            lpfn_glsl_params: import.lpfn_glsl_params.clone(),
            needs_vmctx: false,
            sret: false,
        });
        import_map.insert(import.key.clone(), callee);
    }

    for function in &module.functions {
        let lowered = lower_function(function, &import_map)?;
        mb.add_function(lowered);
    }
    let ir = mb.finish();
    if let Err(errors) = lpir::validate_module(&ir) {
        let message = errors.first().map_or_else(
            || String::from("unknown LPIR validation error"),
            ToString::to_string,
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
    import_map: &BTreeMap<ImportKey, CalleeRef>,
) -> Result<lpir::IrFunction, Diagnostic> {
    let return_types = scalar_ir_types(&function.return_ty)?;
    let mut fb = FunctionBuilder::new(&function.name, &return_types);
    let mut params = Vec::new();
    for param in &function.params {
        let mut lanes = Vec::new();
        for ty in scalar_ir_types(&param.ty)? {
            lanes.push(fb.add_param(ty));
        }
        params.push(LowerValue {
            ty: param.ty.clone(),
            lanes,
        });
    }
    let mut locals = Vec::new();
    for local in &function.body.locals {
        let mut lanes = Vec::new();
        for ty in scalar_ir_types(&local.ty)? {
            lanes.push(fb.alloc_vreg(ty));
        }
        locals.push(LowerValue {
            ty: local.ty.clone(),
            lanes,
        });
    }
    let mut ctx = LowerCtx {
        fb,
        params,
        locals,
        import_map,
    };
    lower_statements(&mut ctx, &function.body.statements)?;
    Ok(ctx.fb.finish())
}

struct LowerCtx<'a> {
    fb: FunctionBuilder,
    params: Vec<LowerValue>,
    locals: Vec<LowerValue>,
    import_map: &'a BTreeMap<ImportKey, CalleeRef>,
}

#[derive(Debug, Clone)]
struct LowerValue {
    ty: LpsType,
    lanes: Vec<VReg>,
}

fn lower_statements(ctx: &mut LowerCtx<'_>, statements: &[HirStmt]) -> Result<(), Diagnostic> {
    for stmt in statements {
        lower_stmt(ctx, stmt)?;
    }
    Ok(())
}

fn lower_stmt(ctx: &mut LowerCtx<'_>, stmt: &HirStmt) -> Result<(), Diagnostic> {
    match stmt {
        HirStmt::Let { local, init } => {
            let value = lower_expr(ctx, init)?;
            copy_value(ctx, ctx.locals[*local].clone(), value, init.span)
        }
        HirStmt::Assign { local, value } => {
            let span = value.span;
            let value = lower_expr(ctx, value)?;
            copy_value(ctx, ctx.locals[*local].clone(), value, span)
        }
        HirStmt::If {
            condition,
            accept,
            reject,
        } => {
            let cond = lower_expr(ctx, condition)?;
            let cond = single_lane(condition.span, &cond)?;
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
            let cond = lower_expr(ctx, condition)?;
            let cond = single_lane(condition.span, &cond)?;
            ctx.fb.push(LpirOp::BrIfNot { cond });
            lower_statements(ctx, body)?;
            ctx.fb.push_continuing();
            lower_statements(ctx, continuing)?;
            ctx.fb.end_loop();
            Ok(())
        }
        HirStmt::While { condition, body } => {
            ctx.fb.push_loop();
            let cond = lower_expr(ctx, condition)?;
            let cond = single_lane(condition.span, &cond)?;
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
            let cond = lower_expr(ctx, condition)?;
            let cond = single_lane(condition.span, &cond)?;
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
            let _ = lower_expr(ctx, expr)?;
            Ok(())
        }
        HirStmt::Return(expr) => {
            let value = lower_expr(ctx, expr)?;
            ctx.fb.push_return(&value.lanes);
            Ok(())
        }
    }
}

fn lower_expr(ctx: &mut LowerCtx<'_>, expr: &HirExpr) -> Result<LowerValue, Diagnostic> {
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
        HirExprKind::Param { index } => ctx.params.get(*index).cloned().ok_or_else(|| {
            Diagnostic::error(
                expr.span,
                format!("parameter index {index} is out of range"),
            )
        }),
        HirExprKind::Local { index } => ctx.locals.get(*index).cloned().ok_or_else(|| {
            Diagnostic::error(expr.span, format!("local index {index} is out of range"))
        }),
        HirExprKind::Uniform {
            name: _,
            byte_offset,
        } => lower_uniform_load(ctx, expr.span, *byte_offset, &expr.ty),
        HirExprKind::Constructor { args } => {
            let mut lanes = Vec::new();
            if args.len() == 1
                && scalar_lane_count(&expr.ty) > 1
                && scalar_lane_count(&args[0].ty) == 1
            {
                let value = lower_expr(ctx, &args[0])?;
                let lane = single_lane(args[0].span, &value)?;
                lanes.resize(scalar_lane_count(&expr.ty), lane);
            } else {
                for arg in args {
                    lanes.extend(lower_expr(ctx, arg)?.lanes);
                }
            }
            Ok(LowerValue {
                ty: expr.ty.clone(),
                lanes,
            })
        }
        HirExprKind::Cast { expr: inner } => {
            let inner = lower_expr(ctx, inner)?;
            lower_cast(ctx, expr.span, inner, &expr.ty)
        }
        HirExprKind::Swizzle { base, lanes } => {
            let base = lower_expr(ctx, base)?;
            let out = lanes.iter().map(|i| base.lanes[*i]).collect::<Vec<_>>();
            Ok(LowerValue {
                ty: expr.ty.clone(),
                lanes: out,
            })
        }
        HirExprKind::Builtin { kind, args } => lower_builtin(ctx, expr.span, *kind, args, &expr.ty),
        HirExprKind::UserCall { function, args } => {
            let mut arg_lanes = vec![VMCTX_VREG];
            for arg in args {
                arg_lanes.extend(lower_expr(ctx, arg)?.lanes);
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
            if matches!(import, ImportKey::Glsl { .. })
                && args.len() == 1
                && scalar_lane_count(&expr.ty) > 1
            {
                let arg = lower_expr(ctx, &args[0])?;
                let mut results = Vec::new();
                for lane in arg.lanes {
                    let dst = ctx.fb.alloc_vreg(IrType::F32);
                    ctx.fb.push_call(callee, &[lane], &[dst]);
                    results.push(dst);
                }
                return Ok(LowerValue {
                    ty: expr.ty.clone(),
                    lanes: results,
                });
            }
            let mut arg_lanes = Vec::new();
            for arg in args {
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
        HirExprKind::Unary { op, expr: inner } => {
            let inner = lower_expr(ctx, inner)?;
            match (op, inner.ty.clone()) {
                (UnaryOp::Neg, LpsType::Float) => {
                    let src = single_lane(expr.span, &inner)?;
                    let dst = ctx.fb.alloc_vreg(IrType::F32);
                    ctx.fb.push(LpirOp::Fneg { dst, src });
                    Ok(LowerValue {
                        ty: LpsType::Float,
                        lanes: vec![dst],
                    })
                }
                (UnaryOp::Neg, LpsType::Int) => {
                    let src = single_lane(expr.span, &inner)?;
                    let dst = ctx.fb.alloc_vreg(IrType::I32);
                    ctx.fb.push(LpirOp::Ineg { dst, src });
                    Ok(LowerValue {
                        ty: LpsType::Int,
                        lanes: vec![dst],
                    })
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
            let lhs = lower_expr(ctx, lhs)?;
            let rhs = lower_expr(ctx, rhs)?;
            lower_binary(ctx, expr.span, *op, lhs, rhs, &expr.ty)
        }
        HirExprKind::Sequence { first, second } => {
            let _ = lower_expr(ctx, first)?;
            lower_expr(ctx, second)
        }
        HirExprKind::Conditional {
            condition,
            accept,
            reject,
        } => {
            let condition = lower_expr(ctx, condition)?;
            let accept = lower_expr(ctx, accept)?;
            let reject = lower_expr(ctx, reject)?;
            lower_select(ctx, expr.span, condition, accept, reject, &expr.ty)
        }
        HirExprKind::Assign { local, value } => {
            let value = lower_expr(ctx, value)?;
            let dst = ctx.locals.get(*local).cloned().ok_or_else(|| {
                Diagnostic::error(expr.span, format!("local index {local} is out of range"))
            })?;
            copy_value(ctx, dst, value.clone(), expr.span)?;
            Ok(value)
        }
        HirExprKind::IncDec { local, op, prefix } => {
            lower_inc_dec(ctx, expr.span, *local, *op, *prefix)
        }
    }
}

fn lower_import_call_with_out(
    ctx: &mut LowerCtx<'_>,
    span: Span,
    callee: CalleeRef,
    args: &[HirExpr],
    out: &crate::hir::HirOutArg,
    result_ty: &LpsType,
) -> Result<LowerValue, Diagnostic> {
    let out_lanes = scalar_lane_count(&out.ty);
    let slot = ctx.fb.alloc_slot(out_lanes as u32 * 4);
    let addr = ctx.fb.alloc_vreg(IrType::I32);
    ctx.fb.push(LpirOp::SlotAddr { dst: addr, slot });

    let mut arg_lanes = Vec::new();
    let mut value_arg = 0usize;
    for arg_index in 0..(args.len() + 1) {
        if arg_index == out.arg_index {
            arg_lanes.push(addr);
        } else {
            let arg = args.get(value_arg).ok_or_else(|| {
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

    let Some(local) = ctx.locals.get(out.local).cloned() else {
        return Err(Diagnostic::error(span, "internal lpfn out local missing"));
    };
    if local.lanes.len() != out_lanes {
        return Err(Diagnostic::error(
            span,
            "internal lpfn out local width mismatch",
        ));
    }
    for (i, dst) in local.lanes.iter().enumerate() {
        let tmp = ctx.fb.alloc_vreg(IrType::F32);
        ctx.fb.push(LpirOp::Load {
            dst: tmp,
            base: addr,
            offset: i as u32 * 4,
        });
        ctx.fb.push(LpirOp::Copy {
            dst: *dst,
            src: tmp,
        });
    }

    Ok(LowerValue {
        ty: result_ty.clone(),
        lanes: results,
    })
}

fn lower_uniform_load(
    ctx: &mut LowerCtx<'_>,
    span: Span,
    byte_offset: u32,
    ty: &LpsType,
) -> Result<LowerValue, Diagnostic> {
    let ir_types = scalar_ir_types(ty)?;
    let mut lanes = Vec::new();
    for (i, ir_ty) in ir_types.iter().enumerate() {
        let dst = ctx.fb.alloc_vreg(*ir_ty);
        ctx.fb.push(LpirOp::Load {
            dst,
            base: VMCTX_VREG,
            offset: byte_offset.saturating_add((i as u32).saturating_mul(4)),
        });
        lanes.push(dst);
    }
    if lanes.len() != scalar_lane_count(ty) {
        return Err(Diagnostic::error(span, "uniform lane count mismatch"));
    }
    Ok(LowerValue {
        ty: ty.clone(),
        lanes,
    })
}

fn lower_builtin(
    ctx: &mut LowerCtx<'_>,
    span: Span,
    kind: BuiltinKind,
    args: &[HirExpr],
    result_ty: &LpsType,
) -> Result<LowerValue, Diagnostic> {
    let values = args
        .iter()
        .map(|arg| lower_expr(ctx, arg))
        .collect::<Result<Vec<_>, _>>()?;
    let width = scalar_lane_count(result_ty);
    let mut lanes = Vec::new();
    for i in 0..width {
        let lane = match kind {
            BuiltinKind::Abs => {
                lower_unary_float_lane(ctx, span, result_ty, &values[0], i, UnaryFloatOp::Abs)?
            }
            BuiltinKind::All | BuiltinKind::Any | BuiltinKind::Not => {
                return lower_bool_builtin(ctx, span, kind, &values[0], result_ty);
            }
            BuiltinKind::Equal => {
                return lower_binary(
                    ctx,
                    span,
                    BinaryOp::Eq,
                    values[0].clone(),
                    values[1].clone(),
                    result_ty,
                );
            }
            BuiltinKind::NotEqual => {
                return lower_binary(
                    ctx,
                    span,
                    BinaryOp::Ne,
                    values[0].clone(),
                    values[1].clone(),
                    result_ty,
                );
            }
            BuiltinKind::Floor => {
                lower_unary_float_lane(ctx, span, result_ty, &values[0], i, UnaryFloatOp::Floor)?
            }
            BuiltinKind::Fract => {
                let x = lane_at(&values[0], i);
                let f = ctx.fb.alloc_vreg(IrType::F32);
                let dst = ctx.fb.alloc_vreg(IrType::F32);
                ctx.fb.push(LpirOp::Ffloor { dst: f, src: x });
                ctx.fb.push(LpirOp::Fsub {
                    dst,
                    lhs: x,
                    rhs: f,
                });
                dst
            }
            BuiltinKind::Min => {
                lower_binary_float_lane(ctx, &values[0], &values[1], i, BinaryFloatOp::Min)
            }
            BuiltinKind::Max => {
                lower_binary_float_lane(ctx, &values[0], &values[1], i, BinaryFloatOp::Max)
            }
            BuiltinKind::Mod => lower_mod_lane(ctx, &values[0], &values[1], i),
            BuiltinKind::Clamp => {
                let maxed =
                    lower_binary_float_lane(ctx, &values[0], &values[1], i, BinaryFloatOp::Max);
                let hi = lane_at(&values[2], i);
                let dst = ctx.fb.alloc_vreg(IrType::F32);
                ctx.fb.push(LpirOp::Fmin {
                    dst,
                    lhs: maxed,
                    rhs: hi,
                });
                dst
            }
            BuiltinKind::Mix => lower_mix_lane(ctx, &values[0], &values[1], &values[2], i),
            BuiltinKind::Smoothstep => {
                lower_smoothstep_lane(ctx, &values[0], &values[1], &values[2], i)
            }
        };
        lanes.push(lane);
    }
    Ok(LowerValue {
        ty: result_ty.clone(),
        lanes,
    })
}

fn lower_bool_builtin(
    ctx: &mut LowerCtx<'_>,
    span: Span,
    kind: BuiltinKind,
    value: &LowerValue,
    result_ty: &LpsType,
) -> Result<LowerValue, Diagnostic> {
    if scalar_base_type(&value.ty) != Some(LpsType::Bool) {
        return Err(Diagnostic::error(span, "bool builtin expects bool lanes"));
    }
    match kind {
        BuiltinKind::All | BuiltinKind::Any => {
            let Some(mut acc) = value.lanes.first().copied() else {
                return Err(Diagnostic::error(span, "bool reduction has no lanes"));
            };
            for lane in value.lanes.iter().skip(1) {
                let dst = ctx.fb.alloc_vreg(IrType::I32);
                match kind {
                    BuiltinKind::All => ctx.fb.push(LpirOp::Iand {
                        dst,
                        lhs: acc,
                        rhs: *lane,
                    }),
                    BuiltinKind::Any => ctx.fb.push(LpirOp::Ior {
                        dst,
                        lhs: acc,
                        rhs: *lane,
                    }),
                    _ => unreachable!(),
                }
                acc = dst;
            }
            Ok(LowerValue {
                ty: result_ty.clone(),
                lanes: vec![acc],
            })
        }
        BuiltinKind::Not => {
            let mut lanes = Vec::new();
            for lane in &value.lanes {
                let zero = ctx.fb.alloc_vreg(IrType::I32);
                let dst = ctx.fb.alloc_vreg(IrType::I32);
                ctx.fb.push(LpirOp::IconstI32 {
                    dst: zero,
                    value: 0,
                });
                ctx.fb.push(LpirOp::Ieq {
                    dst,
                    lhs: *lane,
                    rhs: zero,
                });
                lanes.push(dst);
            }
            Ok(LowerValue {
                ty: result_ty.clone(),
                lanes,
            })
        }
        _ => Err(Diagnostic::error(span, "unsupported bool builtin")),
    }
}

fn lower_binary(
    ctx: &mut LowerCtx<'_>,
    span: Span,
    op: BinaryOp,
    lhs: LowerValue,
    rhs: LowerValue,
    result_ty: &LpsType,
) -> Result<LowerValue, Diagnostic> {
    if is_logical(op) {
        let lhs_lane = single_lane(span, &lhs)?;
        let rhs_lane = single_lane(span, &rhs)?;
        let dst = ctx.fb.alloc_vreg(IrType::I32);
        let op = match op {
            BinaryOp::LogicalAnd => LpirOp::Iand {
                dst,
                lhs: lhs_lane,
                rhs: rhs_lane,
            },
            BinaryOp::LogicalOr => LpirOp::Ior {
                dst,
                lhs: lhs_lane,
                rhs: rhs_lane,
            },
            BinaryOp::LogicalXor => LpirOp::Ixor {
                dst,
                lhs: lhs_lane,
                rhs: rhs_lane,
            },
            _ => unreachable!(),
        };
        ctx.fb.push(op);
        return Ok(LowerValue {
            ty: LpsType::Bool,
            lanes: vec![dst],
        });
    }
    if is_comparison(op) {
        if matches!(op, BinaryOp::Eq | BinaryOp::Ne)
            && *result_ty == LpsType::Bool
            && lhs.lanes.len() > 1
        {
            let component_ty = LpsType::vector_type(&LpsType::Bool, lhs.lanes.len())
                .ok_or_else(|| Diagnostic::error(span, "unsupported aggregate comparison width"))?;
            let components = lower_binary(ctx, span, op, lhs, rhs, &component_ty)?;
            let reduction = if op == BinaryOp::Eq {
                BuiltinKind::All
            } else {
                BuiltinKind::Any
            };
            return lower_bool_builtin(ctx, span, reduction, &components, &LpsType::Bool);
        }
        let width = scalar_lane_count(result_ty);
        let mut lanes = Vec::new();
        for i in 0..width {
            let lhs_lane = lane_at(&lhs, i);
            let rhs_lane = lane_at(&rhs, i);
            let dst = ctx.fb.alloc_vreg(IrType::I32);
            let base_ty = scalar_base_type(&lhs.ty).unwrap_or_else(|| lhs.ty.clone());
            let op = match base_ty {
                LpsType::Float => match op {
                    BinaryOp::Lt => LpirOp::Flt {
                        dst,
                        lhs: lhs_lane,
                        rhs: rhs_lane,
                    },
                    BinaryOp::Le => LpirOp::Fle {
                        dst,
                        lhs: lhs_lane,
                        rhs: rhs_lane,
                    },
                    BinaryOp::Gt => LpirOp::Fgt {
                        dst,
                        lhs: lhs_lane,
                        rhs: rhs_lane,
                    },
                    BinaryOp::Ge => LpirOp::Fge {
                        dst,
                        lhs: lhs_lane,
                        rhs: rhs_lane,
                    },
                    BinaryOp::Eq => LpirOp::Feq {
                        dst,
                        lhs: lhs_lane,
                        rhs: rhs_lane,
                    },
                    BinaryOp::Ne => LpirOp::Fne {
                        dst,
                        lhs: lhs_lane,
                        rhs: rhs_lane,
                    },
                    _ => unreachable!(),
                },
                LpsType::UInt => match op {
                    BinaryOp::Lt => LpirOp::IltU {
                        dst,
                        lhs: lhs_lane,
                        rhs: rhs_lane,
                    },
                    BinaryOp::Le => LpirOp::IleU {
                        dst,
                        lhs: lhs_lane,
                        rhs: rhs_lane,
                    },
                    BinaryOp::Gt => LpirOp::IgtU {
                        dst,
                        lhs: lhs_lane,
                        rhs: rhs_lane,
                    },
                    BinaryOp::Ge => LpirOp::IgeU {
                        dst,
                        lhs: lhs_lane,
                        rhs: rhs_lane,
                    },
                    BinaryOp::Eq => LpirOp::Ieq {
                        dst,
                        lhs: lhs_lane,
                        rhs: rhs_lane,
                    },
                    BinaryOp::Ne => LpirOp::Ine {
                        dst,
                        lhs: lhs_lane,
                        rhs: rhs_lane,
                    },
                    _ => unreachable!(),
                },
                _ => match op {
                    BinaryOp::Lt => LpirOp::IltS {
                        dst,
                        lhs: lhs_lane,
                        rhs: rhs_lane,
                    },
                    BinaryOp::Le => LpirOp::IleS {
                        dst,
                        lhs: lhs_lane,
                        rhs: rhs_lane,
                    },
                    BinaryOp::Gt => LpirOp::IgtS {
                        dst,
                        lhs: lhs_lane,
                        rhs: rhs_lane,
                    },
                    BinaryOp::Ge => LpirOp::IgeS {
                        dst,
                        lhs: lhs_lane,
                        rhs: rhs_lane,
                    },
                    BinaryOp::Eq => LpirOp::Ieq {
                        dst,
                        lhs: lhs_lane,
                        rhs: rhs_lane,
                    },
                    BinaryOp::Ne => LpirOp::Ine {
                        dst,
                        lhs: lhs_lane,
                        rhs: rhs_lane,
                    },
                    _ => unreachable!(),
                },
            };
            ctx.fb.push(op);
            lanes.push(dst);
        }
        return Ok(LowerValue {
            ty: result_ty.clone(),
            lanes,
        });
    }
    let width = scalar_lane_count(result_ty);
    let mut lanes = Vec::new();
    for i in 0..width {
        let l = lane_at(&lhs, i);
        let r = lane_at(&rhs, i);
        let base_ty = scalar_base_type(result_ty).unwrap_or_else(|| result_ty.clone());
        let dst = match base_ty {
            LpsType::Float => {
                let dst = ctx.fb.alloc_vreg(IrType::F32);
                let op = match op {
                    BinaryOp::Add => LpirOp::Fadd {
                        dst,
                        lhs: l,
                        rhs: r,
                    },
                    BinaryOp::Sub => LpirOp::Fsub {
                        dst,
                        lhs: l,
                        rhs: r,
                    },
                    BinaryOp::Mul => LpirOp::Fmul {
                        dst,
                        lhs: l,
                        rhs: r,
                    },
                    BinaryOp::Div => LpirOp::Fdiv {
                        dst,
                        lhs: l,
                        rhs: r,
                    },
                    _ => return Err(Diagnostic::error(span, "unsupported float binary op")),
                };
                ctx.fb.push(op);
                dst
            }
            LpsType::Int | LpsType::UInt | LpsType::Bool => {
                let dst = ctx.fb.alloc_vreg(IrType::I32);
                let op = match op {
                    BinaryOp::Add => LpirOp::Iadd {
                        dst,
                        lhs: l,
                        rhs: r,
                    },
                    BinaryOp::Sub => LpirOp::Isub {
                        dst,
                        lhs: l,
                        rhs: r,
                    },
                    BinaryOp::Mul => LpirOp::Imul {
                        dst,
                        lhs: l,
                        rhs: r,
                    },
                    BinaryOp::Div if base_ty == LpsType::UInt => LpirOp::IdivU {
                        dst,
                        lhs: l,
                        rhs: r,
                    },
                    BinaryOp::Div => LpirOp::IdivS {
                        dst,
                        lhs: l,
                        rhs: r,
                    },
                    BinaryOp::Mod if base_ty == LpsType::UInt => LpirOp::IremU {
                        dst,
                        lhs: l,
                        rhs: r,
                    },
                    BinaryOp::Mod => LpirOp::IremS {
                        dst,
                        lhs: l,
                        rhs: r,
                    },
                    _ => return Err(Diagnostic::error(span, "unsupported integer binary op")),
                };
                ctx.fb.push(op);
                dst
            }
            _ => return Err(Diagnostic::error(span, "unsupported binary result type")),
        };
        lanes.push(dst);
    }
    Ok(LowerValue {
        ty: result_ty.clone(),
        lanes,
    })
}

fn lower_cast(
    ctx: &mut LowerCtx<'_>,
    span: Span,
    value: LowerValue,
    target_ty: &LpsType,
) -> Result<LowerValue, Diagnostic> {
    let src_base = scalar_base_type(&value.ty).ok_or_else(|| {
        Diagnostic::error(span, format!("unsupported cast source {:?}", value.ty))
    })?;
    let dst_base = scalar_base_type(target_ty)
        .ok_or_else(|| Diagnostic::error(span, format!("unsupported cast target {target_ty:?}")))?;
    if value.lanes.len() != scalar_lane_count(target_ty) {
        return Err(Diagnostic::error(span, "cast lane count mismatch"));
    }
    let dst_types = scalar_ir_types(target_ty)?;
    let mut lanes = Vec::new();
    for (src, dst_ty) in value.lanes.iter().zip(dst_types.iter()) {
        let dst = lower_scalar_cast(ctx, span, *src, &src_base, &dst_base, *dst_ty)?;
        lanes.push(dst);
    }
    Ok(LowerValue {
        ty: target_ty.clone(),
        lanes,
    })
}

fn lower_scalar_cast(
    ctx: &mut LowerCtx<'_>,
    span: Span,
    src: VReg,
    src_ty: &LpsType,
    dst_ty: &LpsType,
    dst_ir_ty: IrType,
) -> Result<VReg, Diagnostic> {
    let dst = ctx.fb.alloc_vreg(dst_ir_ty);
    match (src_ty, dst_ty) {
        (LpsType::Float, LpsType::Float)
        | (LpsType::Int, LpsType::Int)
        | (LpsType::UInt, LpsType::UInt)
        | (LpsType::Bool, LpsType::Bool)
        | (LpsType::Bool, LpsType::Int)
        | (LpsType::Bool, LpsType::UInt)
        | (LpsType::Int, LpsType::UInt)
        | (LpsType::UInt, LpsType::Int) => ctx.fb.push(LpirOp::Copy { dst, src }),
        (LpsType::Int, LpsType::Float) | (LpsType::Bool, LpsType::Float) => {
            ctx.fb.push(LpirOp::ItofS { dst, src });
        }
        (LpsType::UInt, LpsType::Float) => {
            ctx.fb.push(LpirOp::ItofU { dst, src });
        }
        (LpsType::Float, LpsType::Int) => {
            ctx.fb.push(LpirOp::FtoiSatS { dst, src });
        }
        (LpsType::Float, LpsType::UInt) => {
            ctx.fb.push(LpirOp::FtoiSatU { dst, src });
        }
        (LpsType::Int | LpsType::UInt, LpsType::Bool) => {
            let zero = ctx.fb.alloc_vreg(IrType::I32);
            ctx.fb.push(LpirOp::IconstI32 {
                dst: zero,
                value: 0,
            });
            ctx.fb.push(LpirOp::Ine {
                dst,
                lhs: src,
                rhs: zero,
            });
        }
        (LpsType::Float, LpsType::Bool) => {
            let zero = ctx.fb.alloc_vreg(IrType::F32);
            ctx.fb.push(LpirOp::FconstF32 {
                dst: zero,
                value: 0.0,
            });
            ctx.fb.push(LpirOp::Fne {
                dst,
                lhs: src,
                rhs: zero,
            });
        }
        _ => {
            return Err(Diagnostic::error(
                span,
                format!("unsupported scalar cast {src_ty:?} to {dst_ty:?}"),
            ));
        }
    }
    Ok(dst)
}

fn lower_select(
    ctx: &mut LowerCtx<'_>,
    span: Span,
    condition: LowerValue,
    accept: LowerValue,
    reject: LowerValue,
    result_ty: &LpsType,
) -> Result<LowerValue, Diagnostic> {
    let cond = single_lane(span, &condition)?;
    if accept.lanes.len() != reject.lanes.len() {
        return Err(Diagnostic::error(span, "ternary arm lane count mismatch"));
    }
    let result_types = scalar_ir_types(result_ty)?;
    if result_types.len() != accept.lanes.len() {
        return Err(Diagnostic::error(
            span,
            "ternary result lane count mismatch",
        ));
    }
    let mut lanes = Vec::new();
    for ((if_true, if_false), ty) in accept
        .lanes
        .iter()
        .zip(reject.lanes.iter())
        .zip(result_types.iter())
    {
        let dst = ctx.fb.alloc_vreg(*ty);
        ctx.fb.push(LpirOp::Select {
            dst,
            cond,
            if_true: *if_true,
            if_false: *if_false,
        });
        lanes.push(dst);
    }
    Ok(LowerValue {
        ty: result_ty.clone(),
        lanes,
    })
}

fn lower_inc_dec(
    ctx: &mut LowerCtx<'_>,
    span: Span,
    local: usize,
    op: IncDecOp,
    prefix: bool,
) -> Result<LowerValue, Diagnostic> {
    let current =
        ctx.locals.get(local).cloned().ok_or_else(|| {
            Diagnostic::error(span, format!("local index {local} is out of range"))
        })?;
    let old = temp_copy(ctx, &current, span)?;
    let one = one_value(ctx, span, &current.ty)?;
    let binary_op = match op {
        IncDecOp::Increment => BinaryOp::Add,
        IncDecOp::Decrement => BinaryOp::Sub,
    };
    let updated = lower_binary(ctx, span, binary_op, old.clone(), one, &current.ty)?;
    copy_value(ctx, current, updated.clone(), span)?;
    if prefix { Ok(updated) } else { Ok(old) }
}

fn temp_copy(
    ctx: &mut LowerCtx<'_>,
    value: &LowerValue,
    span: Span,
) -> Result<LowerValue, Diagnostic> {
    let mut lanes = Vec::new();
    for (lane, ty) in value.lanes.iter().zip(scalar_ir_types(&value.ty)?) {
        let dst = ctx.fb.alloc_vreg(ty);
        ctx.fb.push(LpirOp::Copy { dst, src: *lane });
        lanes.push(dst);
    }
    if lanes.len() != value.lanes.len() {
        return Err(Diagnostic::error(span, "temporary copy lane mismatch"));
    }
    Ok(LowerValue {
        ty: value.ty.clone(),
        lanes,
    })
}

fn one_value(ctx: &mut LowerCtx<'_>, span: Span, ty: &LpsType) -> Result<LowerValue, Diagnostic> {
    let lane = match ty {
        LpsType::Float => {
            let dst = ctx.fb.alloc_vreg(IrType::F32);
            ctx.fb.push(LpirOp::FconstF32 { dst, value: 1.0 });
            dst
        }
        LpsType::Int => {
            let dst = ctx.fb.alloc_vreg(IrType::I32);
            ctx.fb.push(LpirOp::IconstI32 { dst, value: 1 });
            dst
        }
        LpsType::UInt => {
            let dst = ctx.fb.alloc_vreg(IrType::I32);
            ctx.fb.push(LpirOp::IconstI32 { dst, value: 1 });
            dst
        }
        _ => return Err(Diagnostic::error(span, "unsupported increment type")),
    };
    Ok(LowerValue {
        ty: ty.clone(),
        lanes: vec![lane],
    })
}

fn copy_value(
    ctx: &mut LowerCtx<'_>,
    dst: LowerValue,
    src: LowerValue,
    span: Span,
) -> Result<(), Diagnostic> {
    if dst.lanes.len() != src.lanes.len() {
        return Err(Diagnostic::error(span, "copy lane count mismatch"));
    }
    for (dst, src) in dst.lanes.iter().zip(src.lanes.iter()) {
        ctx.fb.push(LpirOp::Copy {
            dst: *dst,
            src: *src,
        });
    }
    Ok(())
}

#[derive(Debug, Clone, Copy)]
enum UnaryFloatOp {
    Abs,
    Floor,
}

fn lower_unary_float_lane(
    ctx: &mut LowerCtx<'_>,
    span: Span,
    result_ty: &LpsType,
    value: &LowerValue,
    index: usize,
    op: UnaryFloatOp,
) -> Result<VReg, Diagnostic> {
    if scalar_base_type(result_ty) != Some(LpsType::Float) {
        return Err(Diagnostic::error(
            span,
            "builtin currently expects float lanes",
        ));
    }
    let src = lane_at(value, index);
    let dst = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(match op {
        UnaryFloatOp::Abs => LpirOp::Fabs { dst, src },
        UnaryFloatOp::Floor => LpirOp::Ffloor { dst, src },
    });
    Ok(dst)
}

#[derive(Debug, Clone, Copy)]
enum BinaryFloatOp {
    Min,
    Max,
}

fn lower_binary_float_lane(
    ctx: &mut LowerCtx<'_>,
    lhs: &LowerValue,
    rhs: &LowerValue,
    index: usize,
    op: BinaryFloatOp,
) -> VReg {
    let dst = ctx.fb.alloc_vreg(IrType::F32);
    let lhs = lane_at(lhs, index);
    let rhs = lane_at(rhs, index);
    ctx.fb.push(match op {
        BinaryFloatOp::Min => LpirOp::Fmin { dst, lhs, rhs },
        BinaryFloatOp::Max => LpirOp::Fmax { dst, lhs, rhs },
    });
    dst
}

fn lower_mod_lane(
    ctx: &mut LowerCtx<'_>,
    lhs: &LowerValue,
    rhs: &LowerValue,
    index: usize,
) -> VReg {
    let lhs = lane_at(lhs, index);
    let rhs = lane_at(rhs, index);
    let div = ctx.fb.alloc_vreg(IrType::F32);
    let floored = ctx.fb.alloc_vreg(IrType::F32);
    let scaled = ctx.fb.alloc_vreg(IrType::F32);
    let dst = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(LpirOp::Fdiv { dst: div, lhs, rhs });
    ctx.fb.push(LpirOp::Ffloor {
        dst: floored,
        src: div,
    });
    ctx.fb.push(LpirOp::Fmul {
        dst: scaled,
        lhs: rhs,
        rhs: floored,
    });
    ctx.fb.push(LpirOp::Fsub {
        dst,
        lhs,
        rhs: scaled,
    });
    dst
}

fn lower_mix_lane(
    ctx: &mut LowerCtx<'_>,
    x: &LowerValue,
    y: &LowerValue,
    a: &LowerValue,
    index: usize,
) -> VReg {
    let x = lane_at(x, index);
    let y = lane_at(y, index);
    let a = lane_at(a, index);
    let one = fconst(ctx, 1.0);
    let inv = ctx.fb.alloc_vreg(IrType::F32);
    let left = ctx.fb.alloc_vreg(IrType::F32);
    let right = ctx.fb.alloc_vreg(IrType::F32);
    let dst = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(LpirOp::Fsub {
        dst: inv,
        lhs: one,
        rhs: a,
    });
    ctx.fb.push(LpirOp::Fmul {
        dst: left,
        lhs: x,
        rhs: inv,
    });
    ctx.fb.push(LpirOp::Fmul {
        dst: right,
        lhs: y,
        rhs: a,
    });
    ctx.fb.push(LpirOp::Fadd {
        dst,
        lhs: left,
        rhs: right,
    });
    dst
}

fn lower_smoothstep_lane(
    ctx: &mut LowerCtx<'_>,
    edge0: &LowerValue,
    edge1: &LowerValue,
    x: &LowerValue,
    index: usize,
) -> VReg {
    let e0 = lane_at(edge0, index);
    let e1 = lane_at(edge1, index);
    let x = lane_at(x, index);
    let num = ctx.fb.alloc_vreg(IrType::F32);
    let den = ctx.fb.alloc_vreg(IrType::F32);
    let raw_t = ctx.fb.alloc_vreg(IrType::F32);
    let zero = fconst(ctx, 0.0);
    let one = fconst(ctx, 1.0);
    ctx.fb.push(LpirOp::Fsub {
        dst: num,
        lhs: x,
        rhs: e0,
    });
    ctx.fb.push(LpirOp::Fsub {
        dst: den,
        lhs: e1,
        rhs: e0,
    });
    ctx.fb.push(LpirOp::Fdiv {
        dst: raw_t,
        lhs: num,
        rhs: den,
    });
    let low = ctx.fb.alloc_vreg(IrType::F32);
    let t = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(LpirOp::Fmax {
        dst: low,
        lhs: raw_t,
        rhs: zero,
    });
    ctx.fb.push(LpirOp::Fmin {
        dst: t,
        lhs: low,
        rhs: one,
    });
    let two = fconst(ctx, 2.0);
    let three = fconst(ctx, 3.0);
    let tt = ctx.fb.alloc_vreg(IrType::F32);
    let two_t = ctx.fb.alloc_vreg(IrType::F32);
    let factor = ctx.fb.alloc_vreg(IrType::F32);
    let dst = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(LpirOp::Fmul {
        dst: tt,
        lhs: t,
        rhs: t,
    });
    ctx.fb.push(LpirOp::Fmul {
        dst: two_t,
        lhs: two,
        rhs: t,
    });
    ctx.fb.push(LpirOp::Fsub {
        dst: factor,
        lhs: three,
        rhs: two_t,
    });
    ctx.fb.push(LpirOp::Fmul {
        dst,
        lhs: tt,
        rhs: factor,
    });
    dst
}

fn fconst(ctx: &mut LowerCtx<'_>, value: f32) -> VReg {
    let dst = ctx.fb.alloc_vreg(IrType::F32);
    ctx.fb.push(LpirOp::FconstF32 { dst, value });
    dst
}

fn lane_at(value: &LowerValue, index: usize) -> VReg {
    value.lanes[index.min(value.lanes.len().saturating_sub(1))]
}

fn single_lane(span: Span, value: &LowerValue) -> Result<VReg, Diagnostic> {
    match value.lanes.as_slice() {
        [lane] => Ok(*lane),
        _ => Err(Diagnostic::error(span, "expected scalar value")),
    }
}

fn is_comparison(op: BinaryOp) -> bool {
    matches!(
        op,
        BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge | BinaryOp::Eq | BinaryOp::Ne
    )
}

fn is_logical(op: BinaryOp) -> bool {
    matches!(
        op,
        BinaryOp::LogicalAnd | BinaryOp::LogicalOr | BinaryOp::LogicalXor
    )
}
