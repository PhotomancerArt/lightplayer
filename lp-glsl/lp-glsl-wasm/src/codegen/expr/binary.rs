//! Binary expression code generation.

use wasm_encoder::{BlockType, InstructionSink, ValType};

use crate::codegen::context::WasmCodegenContext;
use crate::codegen::expr;
use crate::codegen::numeric::WasmNumericMode;
use crate::codegen::rvalue::WasmRValue;
use crate::options::WasmOptions;
use lp_glsl_frontend::error::GlslDiagnostics;
use lp_glsl_frontend::error::GlslError;
use lp_glsl_frontend::semantic::type_check::infer_binary_result_type;
use lp_glsl_frontend::semantic::types::Type;

/// Emit binary expression with vector and scalar promotion support.
pub fn emit_binary(
    ctx: &mut WasmCodegenContext,
    sink: &mut InstructionSink,
    op: &glsl::syntax::BinaryOp,
    lhs: &glsl::syntax::Expr,
    rhs: &glsl::syntax::Expr,
    options: &WasmOptions,
    span: &glsl::syntax::SourceSpan,
) -> Result<WasmRValue, lp_glsl_frontend::error::GlslDiagnostics> {
    use glsl::syntax::BinaryOp;
    if matches!(op, BinaryOp::And | BinaryOp::Or) {
        return match op {
            BinaryOp::And => emit_logical_and(ctx, sink, lhs, rhs, options),
            BinaryOp::Or => emit_logical_or(ctx, sink, lhs, rhs, options),
            _ => unreachable!(),
        };
    }

    let lhs_ty = crate::codegen::expr::infer_expr_type(ctx, lhs)?;
    let rhs_ty = crate::codegen::expr::infer_expr_type(ctx, rhs)?;

    if lhs_ty.is_vector() || rhs_ty.is_vector() {
        emit_vector_binary(ctx, sink, op, lhs, rhs, &lhs_ty, &rhs_ty, options, span)
    } else {
        let lhs_rv = expr::emit_rvalue(ctx, sink, lhs, options)?;
        let rhs_rv = expr::emit_rvalue(ctx, sink, rhs, options)?;
        let numeric = WasmNumericMode::from(options.float_mode);
        emit_binary_op(ctx, sink, op, &lhs_rv.ty, &rhs_rv.ty, numeric)?;
        let result_ty = infer_binary_result_type(op, &lhs_rv.ty, &rhs_rv.ty, span.clone())
            .map_err(lp_glsl_frontend::error::GlslDiagnostics::from)?;
        Ok(WasmRValue::scalar(result_ty))
    }
}

/// Emit vector binary op (vec+vec, scalar+vec, vec+scalar). Component-wise.
fn emit_vector_binary(
    ctx: &mut WasmCodegenContext,
    sink: &mut InstructionSink,
    op: &glsl::syntax::BinaryOp,
    lhs: &glsl::syntax::Expr,
    rhs: &glsl::syntax::Expr,
    lhs_ty: &Type,
    rhs_ty: &Type,
    options: &WasmOptions,
    span: &glsl::syntax::SourceSpan,
) -> Result<WasmRValue, lp_glsl_frontend::error::GlslDiagnostics> {
    use glsl::syntax::BinaryOp;

    let result_ty = infer_binary_result_type(op, lhs_ty, rhs_ty, span.clone())
        .map_err(lp_glsl_frontend::error::GlslDiagnostics::from)?;

    if matches!(op, BinaryOp::Equal | BinaryOp::NonEqual)
        && lhs_ty.is_vector()
        && rhs_ty.is_vector()
    {
        return emit_vector_equality(ctx, sink, op, lhs, rhs, lhs_ty, options);
    }

    let component_count = result_ty
        .component_count()
        .expect("vector binary needs vector result");
    let base_ty = result_ty.vector_base_type().unwrap();
    let numeric = WasmNumericMode::from(options.float_mode);
    let base = ctx.binary_op_temp_base(&result_ty);

    let (lhs_base, rhs_base) = (base, base + 4);

    if lhs_ty.is_vector() && rhs_ty.is_vector() {
        expr::emit_rvalue(ctx, sink, lhs, options)?;
        for i in (0..component_count).rev() {
            sink.local_set(lhs_base + i as u32);
        }
        expr::emit_rvalue(ctx, sink, rhs, options)?;
        for i in (0..component_count).rev() {
            sink.local_set(rhs_base + i as u32);
        }
    } else if lhs_ty.is_scalar() && rhs_ty.is_vector() {
        let scalar_temp = ctx.get_broadcast_temp(lhs_ty.clone());
        expr::emit_rvalue(ctx, sink, lhs, options)?;
        sink.local_tee(scalar_temp);
        sink.drop();
        expr::emit_rvalue(ctx, sink, rhs, options)?;
        for i in (0..component_count).rev() {
            sink.local_set(rhs_base + i as u32);
        }
        for i in 0..component_count {
            sink.local_get(scalar_temp);
            sink.local_get(rhs_base + i as u32);
            emit_single_binary_op(ctx, sink, op, &base_ty, numeric)?;
        }
        return Ok(WasmRValue::from_type(result_ty));
    } else if lhs_ty.is_vector() && rhs_ty.is_scalar() {
        let scalar_temp = ctx.get_broadcast_temp(rhs_ty.clone());
        expr::emit_rvalue(ctx, sink, lhs, options)?;
        for i in (0..component_count).rev() {
            sink.local_set(lhs_base + i as u32);
        }
        expr::emit_rvalue(ctx, sink, rhs, options)?;
        sink.local_tee(scalar_temp);
        sink.drop();
        for i in 0..component_count {
            sink.local_get(lhs_base + i as u32);
            sink.local_get(scalar_temp);
            emit_single_binary_op(ctx, sink, op, &base_ty, numeric)?;
        }
        return Ok(WasmRValue::from_type(result_ty));
    } else {
        return Err(GlslError::new(
            lp_glsl_frontend::error::ErrorCode::E0400,
            alloc::format!(
                "binary op with mixed vector/scalar: {:?} {:?} {:?}",
                lhs_ty,
                op,
                rhs_ty
            ),
        )
        .into());
    }

    for i in 0..component_count {
        sink.local_get(lhs_base + i as u32);
        sink.local_get(rhs_base + i as u32);
        emit_single_binary_op(ctx, sink, op, &base_ty, numeric)?;
    }
    Ok(WasmRValue::from_type(result_ty))
}

/// Emit vec==vec or vec!=vec (aggregate comparison, returns bool).
fn emit_vector_equality(
    ctx: &mut WasmCodegenContext,
    sink: &mut InstructionSink,
    op: &glsl::syntax::BinaryOp,
    lhs: &glsl::syntax::Expr,
    rhs: &glsl::syntax::Expr,
    vec_ty: &Type,
    options: &WasmOptions,
) -> Result<WasmRValue, lp_glsl_frontend::error::GlslDiagnostics> {
    use glsl::syntax::BinaryOp;

    let component_count = vec_ty.component_count().unwrap();
    let base_ty = vec_ty.vector_base_type().unwrap();
    let numeric = WasmNumericMode::from(options.float_mode);
    let base = ctx.binary_op_temp_base(vec_ty);
    let (lhs_base, rhs_base) = (base, base + 4);

    expr::emit_rvalue(ctx, sink, lhs, options)?;
    for i in (0..component_count).rev() {
        sink.local_set(lhs_base + i as u32);
    }
    expr::emit_rvalue(ctx, sink, rhs, options)?;
    for i in (0..component_count).rev() {
        sink.local_set(rhs_base + i as u32);
    }

    for i in 0..component_count {
        sink.local_get(lhs_base + i as u32);
        sink.local_get(rhs_base + i as u32);
        if matches!(op, BinaryOp::Equal) {
            if base_ty == Type::Float && numeric == WasmNumericMode::Float {
                sink.f32_eq();
            } else {
                sink.i32_eq();
            }
        } else {
            if base_ty == Type::Float && numeric == WasmNumericMode::Float {
                sink.f32_ne();
            } else {
                sink.i32_ne();
            }
        }
    }
    if matches!(op, BinaryOp::Equal) {
        for _ in 0..component_count - 1 {
            sink.i32_and();
        }
    } else {
        for _ in 0..component_count - 1 {
            sink.i32_or();
        }
    }
    Ok(WasmRValue::scalar(Type::Bool))
}

fn emit_single_binary_op(
    ctx: &WasmCodegenContext,
    sink: &mut InstructionSink,
    op: &glsl::syntax::BinaryOp,
    base_ty: &Type,
    numeric: WasmNumericMode,
) -> Result<(), lp_glsl_frontend::error::GlslDiagnostics> {
    emit_binary_op(ctx, sink, op, base_ty, base_ty, numeric)
}

fn is_integer_like(ty: &Type) -> bool {
    matches!(ty, Type::Int | Type::UInt | Type::Bool)
}

/// Q32 fixed-point range: [MIN_FIXED, MAX_FIXED].
const Q32_MAX_FIXED: i32 = 0x7FFF_FFFF;
const Q32_MIN_FIXED: i32 = i32::MIN;

/// Emit binary op with type-aware dispatch.
pub fn emit_binary_op(
    ctx: &WasmCodegenContext,
    sink: &mut InstructionSink,
    op: &glsl::syntax::BinaryOp,
    lhs_ty: &Type,
    rhs_ty: &Type,
    numeric: WasmNumericMode,
) -> Result<(), lp_glsl_frontend::error::GlslDiagnostics> {
    use glsl::syntax::BinaryOp::*;

    let both_int = is_integer_like(lhs_ty) && is_integer_like(rhs_ty);
    let either_float = lhs_ty == &Type::Float || rhs_ty == &Type::Float;

    match op {
        Add => {
            match (both_int, either_float, numeric) {
                (true, _, _) | (_, false, _) => sink.i32_add(),
                (_, true, WasmNumericMode::Q32) => {
                    emit_q32_add_sat(ctx, sink);
                    sink
                }
                (_, true, WasmNumericMode::Float) => sink.f32_add(),
            };
        }
        Sub => {
            match (both_int, either_float, numeric) {
                (true, _, _) | (_, false, _) => sink.i32_sub(),
                (_, true, WasmNumericMode::Q32) => {
                    emit_q32_sub_sat(ctx, sink);
                    sink
                }
                (_, true, WasmNumericMode::Float) => sink.f32_sub(),
            };
        }
        Mult => match (both_int, either_float, numeric) {
            (true, _, _) => {
                sink.i32_mul();
            }
            (_, true, WasmNumericMode::Q32) => {
                emit_q32_mul_sat(ctx, sink)?;
            }
            (_, true, WasmNumericMode::Float) => {
                sink.f32_mul();
            }
            _ => {
                return Err(GlslError::new(
                    lp_glsl_frontend::error::ErrorCode::E0400,
                    "Q32 multiplication requires float operands",
                )
                .into());
            }
        },
        Div => {
            match (both_int, either_float, numeric) {
                (true, _, _) => sink.i32_div_s(),
                (_, true, WasmNumericMode::Q32) => {
                    emit_q32_div(ctx, sink);
                    sink
                }
                (_, true, WasmNumericMode::Float) => sink.f32_div(),
                _ => {
                    return Err(GlslError::new(
                        lp_glsl_frontend::error::ErrorCode::E0400,
                        "Q32 division requires float operands",
                    )
                    .into());
                }
            };
        }
        Mod => {
            match (both_int, either_float, numeric) {
                (true, _, _) | (_, true, WasmNumericMode::Q32) => sink.i32_rem_s(),
                (_, true, WasmNumericMode::Float) => {
                    return Err(GlslError::new(
                        lp_glsl_frontend::error::ErrorCode::E0400,
                        "float modulo: use mod() builtin",
                    )
                    .into());
                }
                _ => {
                    return Err(GlslError::new(
                        lp_glsl_frontend::error::ErrorCode::E0400,
                        "modulo requires integer or Q32 operands",
                    )
                    .into());
                }
            };
        }
        Equal => {
            if either_float && numeric == WasmNumericMode::Float {
                sink.f32_eq();
            } else {
                sink.i32_eq();
            }
        }
        NonEqual => {
            if either_float && numeric == WasmNumericMode::Float {
                sink.f32_ne();
            } else {
                sink.i32_ne();
            }
        }
        LT => {
            if either_float && numeric == WasmNumericMode::Float {
                sink.f32_lt();
            } else {
                sink.i32_lt_s();
            }
        }
        GT => {
            if either_float && numeric == WasmNumericMode::Float {
                sink.f32_gt();
            } else {
                sink.i32_gt_s();
            }
        }
        LTE => {
            if either_float && numeric == WasmNumericMode::Float {
                sink.f32_le();
            } else {
                sink.i32_le_s();
            }
        }
        GTE => {
            if either_float && numeric == WasmNumericMode::Float {
                sink.f32_ge();
            } else {
                sink.i32_ge_s();
            }
        }
        And | Or => {
            return Err(GlslError::new(
                lp_glsl_frontend::error::ErrorCode::E0400,
                "logical And/Or require short-circuit; use emit_logical_and/or",
            )
            .into());
        }
        Xor => {
            if both_int {
                sink.i32_xor();
            } else {
                return Err(GlslError::new(
                    lp_glsl_frontend::error::ErrorCode::E0400,
                    "Xor requires bool operands",
                )
                .into());
            }
        }
        _ => {
            return Err(GlslError::new(
                lp_glsl_frontend::error::ErrorCode::E0400,
                alloc::format!("binary op {:?} not supported", op),
            )
            .into());
        }
    };
    Ok(())
}

fn emit_q32_add_sat(ctx: &WasmCodegenContext, sink: &mut InstructionSink) {
    // Use vector conv scratch — must not alias `binary_op_i32_base`, which holds
    // per-component operands during vector binops and inline builtins like `mix`.
    let base = ctx
        .vector_conv_i32_base
        .expect("vector_conv i32 temps not allocated");
    sink.local_set(base + 1);
    sink.local_tee(base);
    sink.local_get(base + 1);
    sink.i32_add();
    sink.local_tee(base + 2);
    sink.drop(); // consume sum; if/else will produce final result
    sink.local_get(base);
    sink.i32_const(0);
    sink.i32_gt_s();
    sink.local_get(base + 1);
    sink.i32_const(0);
    sink.i32_gt_s();
    sink.i32_and();
    sink.local_get(base + 2);
    sink.i32_const(0);
    sink.i32_lt_s();
    sink.i32_and();
    sink.if_(BlockType::Result(ValType::I32));
    sink.i32_const(Q32_MAX_FIXED);
    sink.else_();
    sink.local_get(base);
    sink.i32_const(0);
    sink.i32_lt_s();
    sink.local_get(base + 1);
    sink.i32_const(0);
    sink.i32_lt_s();
    sink.i32_and();
    sink.local_get(base + 2);
    sink.i32_const(0);
    sink.i32_ge_s();
    sink.i32_and();
    sink.if_(BlockType::Result(ValType::I32));
    sink.i32_const(Q32_MIN_FIXED);
    sink.else_();
    sink.local_get(base + 2);
    sink.end();
    sink.end();
}

fn emit_q32_sub_sat(ctx: &WasmCodegenContext, sink: &mut InstructionSink) {
    let base = ctx
        .vector_conv_i32_base
        .expect("vector_conv i32 temps not allocated");
    sink.local_set(base + 1);
    sink.local_tee(base);
    sink.local_get(base + 1);
    sink.i32_sub();
    sink.local_tee(base + 2);
    sink.drop(); // consume difference; if/else will produce final result
    sink.local_get(base);
    sink.i32_const(0);
    sink.i32_gt_s();
    sink.local_get(base + 1);
    sink.i32_const(0);
    sink.i32_lt_s();
    sink.i32_and();
    sink.local_get(base + 2);
    sink.i32_const(0);
    sink.i32_lt_s();
    sink.i32_and();
    sink.if_(BlockType::Result(ValType::I32));
    sink.i32_const(Q32_MAX_FIXED);
    sink.else_();
    sink.local_get(base);
    sink.i32_const(0);
    sink.i32_lt_s();
    sink.local_get(base + 1);
    sink.i32_const(0);
    sink.i32_gt_s();
    sink.i32_and();
    sink.local_get(base + 2);
    sink.i32_const(0);
    sink.i32_gt_s();
    sink.i32_and();
    sink.if_(BlockType::Result(ValType::I32));
    sink.i32_const(Q32_MIN_FIXED);
    sink.else_();
    sink.local_get(base + 2);
    sink.end();
    sink.end();
}

/// Fixed-point multiply `(a*b)>>16` with saturation to match `__lp_q32_mul`.
fn emit_q32_mul_sat(
    ctx: &WasmCodegenContext,
    sink: &mut InstructionSink,
) -> Result<(), GlslDiagnostics> {
    let (sa, sb, sw) = ctx.q32_mul_scratch.ok_or_else(|| {
        GlslDiagnostics::from(GlslError::new(
            lp_glsl_frontend::error::ErrorCode::E0400,
            "Q32 mul scratch locals not allocated",
        ))
    })?;
    // Stack: lhs, rhs (rhs on top).
    sink.local_set(sb);
    sink.local_set(sa);
    sink.local_get(sa);
    sink.i32_eqz();
    sink.if_(BlockType::Result(ValType::I32));
    sink.i32_const(0);
    sink.else_();
    sink.local_get(sb);
    sink.i32_eqz();
    sink.if_(BlockType::Result(ValType::I32));
    sink.i32_const(0);
    sink.else_();
    sink.local_get(sa);
    sink.i64_extend_i32_s();
    sink.local_get(sb);
    sink.i64_extend_i32_s();
    sink.i64_mul();
    sink.i64_const(16);
    sink.i64_shr_s();
    sink.local_set(sw);
    sink.local_get(sw);
    sink.i64_const(i64::from(Q32_MAX_FIXED));
    sink.i64_gt_s();
    sink.if_(BlockType::Result(ValType::I32));
    sink.i32_const(Q32_MAX_FIXED);
    sink.else_();
    sink.local_get(sw);
    sink.i64_const(i64::from(Q32_MIN_FIXED));
    sink.i64_lt_s();
    sink.if_(BlockType::Result(ValType::I32));
    sink.i32_const(Q32_MIN_FIXED);
    sink.else_();
    sink.local_get(sw);
    sink.i32_wrap_i64();
    sink.end();
    sink.end();
    sink.end();
    sink.end();
    Ok(())
}

/// Emit short-circuit logical And: lhs && rhs.
pub fn emit_logical_and(
    ctx: &mut WasmCodegenContext,
    sink: &mut InstructionSink,
    lhs: &glsl::syntax::Expr,
    rhs: &glsl::syntax::Expr,
    options: &WasmOptions,
) -> Result<WasmRValue, lp_glsl_frontend::error::GlslDiagnostics> {
    expr::emit_rvalue(ctx, sink, lhs, options)?;
    sink.i32_eqz();
    sink.if_(BlockType::Result(ValType::I32));
    sink.i32_const(0);
    sink.else_();
    expr::emit_rvalue(ctx, sink, rhs, options)?;
    sink.end();
    Ok(WasmRValue::scalar(Type::Bool))
}

/// Emit short-circuit logical Or: lhs || rhs.
pub fn emit_logical_or(
    ctx: &mut WasmCodegenContext,
    sink: &mut InstructionSink,
    lhs: &glsl::syntax::Expr,
    rhs: &glsl::syntax::Expr,
    options: &WasmOptions,
) -> Result<WasmRValue, lp_glsl_frontend::error::GlslDiagnostics> {
    expr::emit_rvalue(ctx, sink, lhs, options)?;
    sink.i32_const(0);
    sink.i32_ne();
    sink.if_(BlockType::Result(ValType::I32));
    sink.i32_const(1);
    sink.else_();
    expr::emit_rvalue(ctx, sink, rhs, options)?;
    sink.end();
    Ok(WasmRValue::scalar(Type::Bool))
}

fn emit_q32_div(ctx: &WasmCodegenContext, sink: &mut InstructionSink) {
    let tmp = ctx
        .broadcast_temp_i32
        .expect("broadcast i32 temp not allocated");
    sink.local_set(tmp);
    sink.i64_extend_i32_s();
    sink.i64_const(16);
    sink.i64_shl();
    sink.local_get(tmp);
    sink.i64_extend_i32_s();
    sink.i64_div_s();
    sink.i32_wrap_i64();
}
