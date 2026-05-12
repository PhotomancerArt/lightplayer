use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use lpir::{FunctionBuilder, IrType, LpirModule, LpirOp, ModuleBuilder, VMCTX_VREG, VReg};
use lps_shared::{LpsModuleSig, LpsType};

use crate::body::{BinaryOp, UnaryOp};
use crate::hir::{
    HirExpr, HirExprKind, HirFunction, HirModule, HirStmt, scalar_base_type, scalar_lane_count,
};
use crate::{Diagnostic, Span};

#[derive(Debug, Clone)]
pub struct LoweredModule {
    pub ir: LpirModule,
    pub meta: LpsModuleSig,
}

pub fn lower_hir(module: HirModule) -> Result<LoweredModule, Diagnostic> {
    let mut mb = ModuleBuilder::new();
    for function in &module.functions {
        let lowered = lower_function(function)?;
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

fn lower_function(function: &HirFunction) -> Result<lpir::IrFunction, Diagnostic> {
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

    for stmt in &function.body.statements {
        match stmt {
            HirStmt::Return(expr) => {
                let value = lower_expr(&mut fb, expr, &params)?;
                if value.ty != function.return_ty {
                    return Err(Diagnostic::error(expr.span, "lowered return type mismatch"));
                }
                fb.push_return(&value.lanes);
            }
        }
    }
    Ok(fb.finish())
}

#[derive(Debug, Clone)]
struct LowerValue {
    ty: LpsType,
    lanes: Vec<VReg>,
}

fn lower_expr(
    fb: &mut FunctionBuilder,
    expr: &HirExpr,
    params: &[LowerValue],
) -> Result<LowerValue, Diagnostic> {
    match &expr.kind {
        HirExprKind::FloatLiteral(v) => {
            let dst = fb.alloc_vreg(IrType::F32);
            fb.push(LpirOp::FconstF32 { dst, value: *v });
            Ok(LowerValue {
                ty: LpsType::Float,
                lanes: vec![dst],
            })
        }
        HirExprKind::IntLiteral(v) => {
            let dst = fb.alloc_vreg(IrType::I32);
            fb.push(LpirOp::IconstI32 { dst, value: *v });
            Ok(LowerValue {
                ty: LpsType::Int,
                lanes: vec![dst],
            })
        }
        HirExprKind::UIntLiteral(v) => {
            let dst = fb.alloc_vreg(IrType::I32);
            fb.push(LpirOp::IconstI32 {
                dst,
                value: i32::from_ne_bytes(v.to_ne_bytes()),
            });
            Ok(LowerValue {
                ty: LpsType::UInt,
                lanes: vec![dst],
            })
        }
        HirExprKind::Param { index } => params.get(*index).cloned().ok_or_else(|| {
            Diagnostic::error(
                expr.span,
                format!("parameter index {index} is out of range"),
            )
        }),
        HirExprKind::Uniform {
            name: _,
            byte_offset,
        } => lower_uniform_load(fb, expr.span, *byte_offset, &expr.ty),
        HirExprKind::Constructor { args } => {
            let mut lanes = Vec::new();
            for arg in args {
                lanes.extend(lower_expr(fb, arg, params)?.lanes);
            }
            Ok(LowerValue {
                ty: expr.ty.clone(),
                lanes,
            })
        }
        HirExprKind::Mod { lhs, rhs } => {
            let lhs = lower_expr(fb, lhs, params)?;
            let rhs = lower_expr(fb, rhs, params)?;
            let (lhs, rhs) = single_float_pair(expr.span, lhs, rhs)?;
            let div = fb.alloc_vreg(IrType::F32);
            let floored = fb.alloc_vreg(IrType::F32);
            let scaled = fb.alloc_vreg(IrType::F32);
            let dst = fb.alloc_vreg(IrType::F32);
            fb.push(LpirOp::Fdiv { dst: div, lhs, rhs });
            fb.push(LpirOp::Ffloor {
                dst: floored,
                src: div,
            });
            fb.push(LpirOp::Fmul {
                dst: scaled,
                lhs: rhs,
                rhs: floored,
            });
            fb.push(LpirOp::Fsub {
                dst,
                lhs,
                rhs: scaled,
            });
            Ok(LowerValue {
                ty: LpsType::Float,
                lanes: vec![dst],
            })
        }
        HirExprKind::Unary { op, expr: inner } => {
            let inner = lower_expr(fb, inner, params)?;
            match (op, inner.ty.clone()) {
                (UnaryOp::Neg, LpsType::Float) => {
                    let src = single_lane(expr.span, &inner)?;
                    let dst = fb.alloc_vreg(IrType::F32);
                    fb.push(LpirOp::Fneg { dst, src });
                    Ok(LowerValue {
                        ty: LpsType::Float,
                        lanes: vec![dst],
                    })
                }
                (UnaryOp::Neg, LpsType::Int) => {
                    let src = single_lane(expr.span, &inner)?;
                    let dst = fb.alloc_vreg(IrType::I32);
                    fb.push(LpirOp::Ineg { dst, src });
                    Ok(LowerValue {
                        ty: LpsType::Int,
                        lanes: vec![dst],
                    })
                }
                _ => Err(Diagnostic::error(expr.span, "unsupported unary lowering")),
            }
        }
        HirExprKind::Binary { op, lhs, rhs } => {
            let lhs = lower_expr(fb, lhs, params)?;
            let rhs = lower_expr(fb, rhs, params)?;
            lower_binary(fb, expr.span, *op, lhs, rhs)
        }
    }
}

fn lower_uniform_load(
    fb: &mut FunctionBuilder,
    span: Span,
    byte_offset: u32,
    ty: &LpsType,
) -> Result<LowerValue, Diagnostic> {
    let ir_types = scalar_ir_types(ty)?;
    let mut lanes = Vec::new();
    for (i, ir_ty) in ir_types.iter().enumerate() {
        let dst = fb.alloc_vreg(*ir_ty);
        fb.push(LpirOp::Load {
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

fn lower_binary(
    fb: &mut FunctionBuilder,
    span: Span,
    op: BinaryOp,
    lhs: LowerValue,
    rhs: LowerValue,
) -> Result<LowerValue, Diagnostic> {
    if lhs.ty != rhs.ty {
        return Err(Diagnostic::error(span, "binary lowering type mismatch"));
    }
    let lhs_lane = single_lane(span, &lhs)?;
    let rhs_lane = single_lane(span, &rhs)?;
    match lhs.ty {
        LpsType::Float => {
            let dst = fb.alloc_vreg(IrType::F32);
            let op = match op {
                BinaryOp::Add => LpirOp::Fadd {
                    dst,
                    lhs: lhs_lane,
                    rhs: rhs_lane,
                },
                BinaryOp::Sub => LpirOp::Fsub {
                    dst,
                    lhs: lhs_lane,
                    rhs: rhs_lane,
                },
                BinaryOp::Mul => LpirOp::Fmul {
                    dst,
                    lhs: lhs_lane,
                    rhs: rhs_lane,
                },
                BinaryOp::Div => LpirOp::Fdiv {
                    dst,
                    lhs: lhs_lane,
                    rhs: rhs_lane,
                },
            };
            fb.push(op);
            Ok(LowerValue {
                ty: LpsType::Float,
                lanes: vec![dst],
            })
        }
        LpsType::Int => {
            let dst = fb.alloc_vreg(IrType::I32);
            let op = match op {
                BinaryOp::Add => LpirOp::Iadd {
                    dst,
                    lhs: lhs_lane,
                    rhs: rhs_lane,
                },
                BinaryOp::Sub => LpirOp::Isub {
                    dst,
                    lhs: lhs_lane,
                    rhs: rhs_lane,
                },
                BinaryOp::Mul => LpirOp::Imul {
                    dst,
                    lhs: lhs_lane,
                    rhs: rhs_lane,
                },
                BinaryOp::Div => LpirOp::IdivS {
                    dst,
                    lhs: lhs_lane,
                    rhs: rhs_lane,
                },
            };
            fb.push(op);
            Ok(LowerValue {
                ty: LpsType::Int,
                lanes: vec![dst],
            })
        }
        _ => Err(Diagnostic::error(
            span,
            "M2 lps-glsl lowers binary arithmetic only for scalar float and int",
        )),
    }
}

fn single_float_pair(
    span: Span,
    lhs: LowerValue,
    rhs: LowerValue,
) -> Result<(VReg, VReg), Diagnostic> {
    if lhs.ty != LpsType::Float || rhs.ty != LpsType::Float {
        return Err(Diagnostic::error(span, "expected scalar float operands"));
    }
    Ok((single_lane(span, &lhs)?, single_lane(span, &rhs)?))
}

fn single_lane(span: Span, value: &LowerValue) -> Result<VReg, Diagnostic> {
    match value.lanes.as_slice() {
        [lane] => Ok(*lane),
        _ => Err(Diagnostic::error(span, "expected scalar value")),
    }
}

fn scalar_ir_types(ty: &LpsType) -> Result<Vec<IrType>, Diagnostic> {
    let Some(base) = scalar_base_type(ty) else {
        return Err(Diagnostic::error(
            Span::new(0, 0),
            format!("M2 lps-glsl cannot scalarize type {ty:?}"),
        ));
    };
    let lane = match base {
        LpsType::Float => IrType::F32,
        LpsType::Int | LpsType::UInt | LpsType::Bool => IrType::I32,
        _ => {
            return Err(Diagnostic::error(
                Span::new(0, 0),
                format!("M2 lps-glsl cannot scalarize type {ty:?}"),
            ));
        }
    };
    Ok(vec![lane; scalar_lane_count(ty)])
}
