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

use crate::body::{BinaryOp, UnaryOp};
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
                _ => Err(Diagnostic::error(expr.span, "unsupported unary lowering")),
            }
        }
        HirExprKind::Binary { op, lhs, rhs } => {
            let lhs = lower_expr(ctx, lhs)?;
            let rhs = lower_expr(ctx, rhs)?;
            lower_binary(ctx, expr.span, *op, lhs, rhs, &expr.ty)
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

fn lower_binary(
    ctx: &mut LowerCtx<'_>,
    span: Span,
    op: BinaryOp,
    lhs: LowerValue,
    rhs: LowerValue,
    result_ty: &LpsType,
) -> Result<LowerValue, Diagnostic> {
    if is_comparison(op) {
        let lhs_lane = single_lane(span, &lhs)?;
        let rhs_lane = single_lane(span, &rhs)?;
        let dst = ctx.fb.alloc_vreg(IrType::I32);
        let op = match lhs.ty {
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
        return Ok(LowerValue {
            ty: LpsType::Bool,
            lanes: vec![dst],
        });
    }
    let width = scalar_lane_count(result_ty);
    let mut lanes = Vec::new();
    for i in 0..width {
        let l = lane_at(&lhs, i);
        let r = lane_at(&rhs, i);
        let dst = match scalar_base_type(result_ty).unwrap_or_else(|| result_ty.clone()) {
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
                    BinaryOp::Div => LpirOp::IdivS {
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
    let src = single_lane(span, &value)?;
    let dst_ty = scalar_ir_types(target_ty)?
        .first()
        .copied()
        .ok_or_else(|| Diagnostic::error(span, "empty cast target"))?;
    let dst = ctx.fb.alloc_vreg(dst_ty);
    match (&value.ty, target_ty) {
        (LpsType::Int, LpsType::Float) => ctx.fb.push(LpirOp::ItofS { dst, src }),
        (LpsType::UInt, LpsType::Float) => ctx.fb.push(LpirOp::ItofU { dst, src }),
        (LpsType::Float, LpsType::Int) => ctx.fb.push(LpirOp::FtoiSatS { dst, src }),
        (LpsType::Float, LpsType::UInt) => ctx.fb.push(LpirOp::FtoiSatU { dst, src }),
        (LpsType::Int, LpsType::UInt) | (LpsType::UInt, LpsType::Int) => {
            ctx.fb.push(LpirOp::Copy { dst, src });
        }
        _ => {
            return Err(Diagnostic::error(
                span,
                format!("unsupported cast {:?} to {target_ty:?}", value.ty),
            ));
        }
    }
    Ok(LowerValue {
        ty: target_ty.clone(),
        lanes: vec![dst],
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
