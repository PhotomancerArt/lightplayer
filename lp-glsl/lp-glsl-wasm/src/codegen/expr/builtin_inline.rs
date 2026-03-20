//! Inline lowering for selected GLSL builtins (no WASM import).

use wasm_encoder::{BlockType, InstructionSink, ValType};

use crate::codegen::context::WasmCodegenContext;
use crate::codegen::expr;
use crate::codegen::expr::infer_expr_type;
use crate::codegen::numeric::WasmNumericMode;
use crate::codegen::rvalue::WasmRValue;
use crate::options::WasmOptions;
use glsl::syntax::{BinaryOp, Expr};
use lp_glsl_frontend::error::{ErrorCode, GlslDiagnostics, GlslError};
use lp_glsl_frontend::semantic::types::Type;

fn is_float_gentype(t: &Type) -> bool {
    matches!(t, Type::Float | Type::Vec2 | Type::Vec3 | Type::Vec4)
}

/// `1.0` as Q16.16 fixed `i32`.
const Q32_ONE: i32 = 65536;
const Q32_TWO: i32 = 2 * 65536;
const Q32_THREE: i32 = 3 * 65536;

/// Builtins lowered inline in the Q32 WASM path — do not add a `builtins` import for these.
pub(crate) fn q32_builtin_import_suppressed(name: &str, argc: usize) -> bool {
    matches!(
        (name, argc),
        ("abs", 1)
            | ("min", 2)
            | ("max", 2)
            | ("clamp", 3)
            | ("mix", 3)
            | ("step", 2)
            | ("sign", 1)
            | ("mod", 2)
            | ("smoothstep", 3)
            | ("floor", 1)
            | ("fract", 1)
    )
}

/// `abs`, `min`, `max`, `clamp` for float `genType` (Q32 i32 or native f32).
pub fn try_emit_inline_builtin(
    ctx: &mut WasmCodegenContext,
    sink: &mut InstructionSink,
    full_call: &Expr,
    name: &str,
    args: &[Expr],
    options: &WasmOptions,
) -> Option<Result<WasmRValue, GlslDiagnostics>> {
    match (name, args.len()) {
        ("abs", 1) => Some(emit_abs(ctx, sink, full_call, &args[0], options)),
        ("min", 2) => Some(emit_min_max(
            ctx, sink, full_call, &args[0], &args[1], true, options,
        )),
        ("max", 2) => Some(emit_min_max(
            ctx, sink, full_call, &args[0], &args[1], false, options,
        )),
        ("clamp", 3) => Some(emit_clamp(
            ctx, sink, full_call, &args[0], &args[1], &args[2], options,
        )),
        ("mix", 3) => Some(emit_mix(
            ctx, sink, full_call, &args[0], &args[1], &args[2], options,
        )),
        ("step", 2) => Some(emit_step(ctx, sink, full_call, &args[0], &args[1], options)),
        ("sign", 1) => Some(emit_sign(ctx, sink, full_call, &args[0], options)),
        ("floor", 1) => Some(emit_floor(ctx, sink, full_call, &args[0], options)),
        ("fract", 1) => Some(emit_fract(ctx, sink, full_call, &args[0], options)),
        ("mod", 2) => Some(emit_mod(ctx, sink, full_call, &args[0], &args[1], options)),
        ("smoothstep", 3) => Some(emit_smoothstep(
            ctx, sink, full_call, &args[0], &args[1], &args[2], options,
        )),
        _ => None,
    }
}

fn scratch_pair(ctx: &WasmCodegenContext) -> Result<(u32, u32), GlslDiagnostics> {
    ctx.minmax_scratch_i32.ok_or_else(|| {
        GlslDiagnostics::from(GlslError::new(
            ErrorCode::E0400,
            "min/max scratch i32 locals not allocated",
        ))
    })
}

fn emit_abs(
    ctx: &mut WasmCodegenContext,
    sink: &mut InstructionSink,
    full_call: &Expr,
    arg: &Expr,
    options: &WasmOptions,
) -> Result<WasmRValue, GlslDiagnostics> {
    let return_ty = infer_expr_type(ctx, full_call)?;
    let arg_ty = infer_expr_type(ctx, arg)?;
    if !is_float_gentype(&arg_ty) {
        return Err(GlslError::new(
            ErrorCode::E0400,
            alloc::format!("`abs` for WASM expects float genType, got {:?}", arg_ty),
        )
        .into());
    }

    let numeric = WasmNumericMode::from(options.float_mode);
    let dim = if arg_ty.is_vector() {
        arg_ty.component_count().unwrap() as u32
    } else {
        1
    };

    match numeric {
        WasmNumericMode::Float => {
            expr::emit_rvalue(ctx, sink, arg, options)?;
            for _ in 0..dim {
                sink.f32_abs();
            }
        }
        WasmNumericMode::Q32 => {
            if dim > 4 {
                return Err(GlslError::new(
                    ErrorCode::E0400,
                    "`abs` vector dimension too large for scratch",
                )
                .into());
            }
            let base = ctx.alloc_i32(dim);
            expr::emit_rvalue(ctx, sink, arg, options)?;
            for i in (0..dim as usize).rev() {
                sink.local_set(base + i as u32);
            }
            let (ta, _) = scratch_pair(ctx)?;
            for i in 0..dim {
                sink.local_get(base + i);
                emit_i32_abs_from_stack(ctx, sink, ta)?;
            }
        }
    }

    Ok(WasmRValue::from_type(return_ty))
}

/// `is_min`: true → `min`, false → `max`.
fn emit_min_max(
    ctx: &mut WasmCodegenContext,
    sink: &mut InstructionSink,
    full_call: &Expr,
    lhs: &Expr,
    rhs: &Expr,
    is_min: bool,
    options: &WasmOptions,
) -> Result<WasmRValue, GlslDiagnostics> {
    let return_ty = infer_expr_type(ctx, full_call)?;
    let lhs_ty = infer_expr_type(ctx, lhs)?;
    let rhs_ty = infer_expr_type(ctx, rhs)?;
    if !is_float_gentype(&lhs_ty) || !is_float_gentype(&rhs_ty) {
        return Err(GlslError::new(
            ErrorCode::E0400,
            alloc::format!(
                "`{}` for WASM expects float genType operands",
                if is_min { "min" } else { "max" }
            ),
        )
        .into());
    }

    let dim = unify_two_float_gentype_dim(&lhs_ty, &rhs_ty)?;
    let slots = slot_span(&lhs_ty, dim) + slot_span(&rhs_ty, dim);

    let numeric = WasmNumericMode::from(options.float_mode);
    match numeric {
        WasmNumericMode::Float => {
            emit_vectorwise_binary_float(
                ctx, sink, lhs, rhs, &lhs_ty, &rhs_ty, dim, is_min, options,
            )?;
        }
        WasmNumericMode::Q32 => {
            let base = ctx.alloc_i32(slots);
            // Evaluate rhs before lhs so nested min/max in rhs cannot clobber lhs scratch.
            let rhs_base = base + slot_span(&lhs_ty, dim);
            store_q32_float_arg(ctx, sink, rhs, &rhs_ty, dim, rhs_base, options)?;
            store_q32_float_arg(ctx, sink, lhs, &lhs_ty, dim, base, options)?;
            for k in 0..dim {
                sink.local_get(base + lhs_offset(&lhs_ty, k));
                sink.local_get(rhs_base + lhs_offset(&rhs_ty, k));
                if is_min {
                    emit_i32_min_max_stack(ctx, sink, true)?;
                } else {
                    emit_i32_min_max_stack(ctx, sink, false)?;
                }
            }
        }
    }

    Ok(WasmRValue::from_type(return_ty))
}

fn emit_clamp(
    ctx: &mut WasmCodegenContext,
    sink: &mut InstructionSink,
    full_call: &Expr,
    x: &Expr,
    lo: &Expr,
    hi: &Expr,
    options: &WasmOptions,
) -> Result<WasmRValue, GlslDiagnostics> {
    let return_ty = infer_expr_type(ctx, full_call)?;
    let x_ty = infer_expr_type(ctx, x)?;
    let lo_ty = infer_expr_type(ctx, lo)?;
    let hi_ty = infer_expr_type(ctx, hi)?;
    if !is_float_gentype(&x_ty) || !is_float_gentype(&lo_ty) || !is_float_gentype(&hi_ty) {
        return Err(GlslError::new(
            ErrorCode::E0400,
            "`clamp` for WASM expects float genType operands",
        )
        .into());
    }

    let dim = unify_three_float_gentype_dim(&x_ty, &lo_ty, &hi_ty)?;
    let total_slots = slot_span(&x_ty, dim) + slot_span(&lo_ty, dim) + slot_span(&hi_ty, dim);

    let numeric = WasmNumericMode::from(options.float_mode);
    match numeric {
        WasmNumericMode::Float => {
            let base = ctx.alloc_f32(total_slots);
            let lo_b = base + slot_span(&x_ty, dim);
            let hi_b = lo_b + slot_span(&lo_ty, dim);
            store_f32_arg(ctx, sink, hi, &hi_ty, dim, hi_b, options)?;
            store_f32_arg(ctx, sink, lo, &lo_ty, dim, lo_b, options)?;
            store_f32_arg(ctx, sink, x, &x_ty, dim, base, options)?;
            for k in 0..dim {
                sink.local_get(base + lhs_offset(&x_ty, k));
                sink.local_get(lo_b + lhs_offset(&lo_ty, k));
                sink.f32_max();
                sink.local_get(hi_b + lhs_offset(&hi_ty, k));
                sink.f32_min();
            }
        }
        WasmNumericMode::Q32 => {
            let base = ctx.alloc_i32(total_slots);
            let lo_b = base + slot_span(&x_ty, dim);
            let hi_b = lo_b + slot_span(&lo_ty, dim);
            store_q32_float_arg(ctx, sink, hi, &hi_ty, dim, hi_b, options)?;
            store_q32_float_arg(ctx, sink, lo, &lo_ty, dim, lo_b, options)?;
            store_q32_float_arg(ctx, sink, x, &x_ty, dim, base, options)?;
            for k in 0..dim {
                sink.local_get(base + lhs_offset(&x_ty, k));
                sink.local_get(lo_b + lhs_offset(&lo_ty, k));
                emit_i32_min_max_stack(ctx, sink, false)?; // max(x, lo)
                sink.local_get(hi_b + lhs_offset(&hi_ty, k));
                emit_i32_min_max_stack(ctx, sink, true)?; // min(..., hi)
            }
        }
    }

    Ok(WasmRValue::from_type(return_ty))
}

fn slot_span(ty: &Type, dim: u32) -> u32 {
    if ty.is_vector() { dim } else { 1 }
}

fn lhs_offset(ty: &Type, k: u32) -> u32 {
    if ty.is_vector() { k } else { 0 }
}

fn unify_two_float_gentype_dim(lhs: &Type, rhs: &Type) -> Result<u32, GlslDiagnostics> {
    let mut dim: u32 = 1;
    for t in [lhs, rhs] {
        match t {
            Type::Float => {}
            Type::Vec2 => dim = dim.max(2),
            Type::Vec3 => dim = dim.max(3),
            Type::Vec4 => dim = dim.max(4),
            _ => {
                return Err(GlslError::new(
                    ErrorCode::E0400,
                    alloc::format!("expected float genType, got {:?}", t),
                )
                .into());
            }
        }
    }
    for t in [lhs, rhs] {
        match t {
            Type::Float => {}
            _ if t.is_vector() => {
                let n = t.component_count().unwrap() as u32;
                if n != dim {
                    return Err(GlslError::new(
                        ErrorCode::E0400,
                        alloc::format!("mismatched vector sizes in min/max (dim {})", dim),
                    )
                    .into());
                }
            }
            _ => {}
        }
    }
    Ok(dim)
}

fn unify_three_float_gentype_dim(x: &Type, lo: &Type, hi: &Type) -> Result<u32, GlslDiagnostics> {
    let mut dim: u32 = 1;
    for t in [x, lo, hi] {
        match t {
            Type::Float => {}
            Type::Vec2 => dim = dim.max(2),
            Type::Vec3 => dim = dim.max(3),
            Type::Vec4 => dim = dim.max(4),
            _ => {
                return Err(GlslError::new(
                    ErrorCode::E0400,
                    alloc::format!("expected float genType, got {:?}", t),
                )
                .into());
            }
        }
    }
    for t in [x, lo, hi] {
        match t {
            Type::Float => {}
            _ if t.is_vector() => {
                let n = t.component_count().unwrap() as u32;
                if n != dim {
                    return Err(GlslError::new(
                        ErrorCode::E0400,
                        alloc::format!("mismatched vector sizes in clamp (dim {})", dim),
                    )
                    .into());
                }
            }
            _ => {}
        }
    }
    Ok(dim)
}

fn store_q32_float_arg(
    ctx: &mut WasmCodegenContext,
    sink: &mut InstructionSink,
    arg: &Expr,
    ty: &Type,
    dim: u32,
    slot_base: u32,
    options: &WasmOptions,
) -> Result<(), GlslDiagnostics> {
    let d = dim as usize;
    if ty.is_vector() {
        expr::emit_rvalue(ctx, sink, arg, options)?;
        for i in (0..d).rev() {
            sink.local_set(slot_base + i as u32);
        }
    } else {
        expr::emit_rvalue(ctx, sink, arg, options)?;
        sink.local_set(slot_base);
        for k in 1..d {
            sink.local_get(slot_base);
            sink.local_set(slot_base + k as u32);
        }
    }
    Ok(())
}

fn store_f32_arg(
    ctx: &mut WasmCodegenContext,
    sink: &mut InstructionSink,
    arg: &Expr,
    ty: &Type,
    dim: u32,
    slot_base: u32,
    options: &WasmOptions,
) -> Result<(), GlslDiagnostics> {
    let d = dim as usize;
    if ty.is_vector() {
        expr::emit_rvalue(ctx, sink, arg, options)?;
        for i in (0..d).rev() {
            sink.local_set(slot_base + i as u32);
        }
    } else {
        expr::emit_rvalue(ctx, sink, arg, options)?;
        sink.local_set(slot_base);
        for k in 1..d {
            sink.local_get(slot_base);
            sink.local_set(slot_base + k as u32);
        }
    }
    Ok(())
}

fn emit_vectorwise_binary_float(
    ctx: &mut WasmCodegenContext,
    sink: &mut InstructionSink,
    lhs: &Expr,
    rhs: &Expr,
    lhs_ty: &Type,
    rhs_ty: &Type,
    dim: u32,
    is_min: bool,
    options: &WasmOptions,
) -> Result<(), GlslDiagnostics> {
    let total = slot_span(lhs_ty, dim) + slot_span(rhs_ty, dim);
    let base = ctx.alloc_f32(total);
    let rhs_base = base + slot_span(lhs_ty, dim);
    store_f32_arg(ctx, sink, rhs, rhs_ty, dim, rhs_base, options)?;
    store_f32_arg(ctx, sink, lhs, lhs_ty, dim, base, options)?;
    for k in 0..dim {
        sink.local_get(base + lhs_offset(lhs_ty, k));
        sink.local_get(rhs_base + lhs_offset(rhs_ty, k));
        if is_min {
            sink.f32_min();
        } else {
            sink.f32_max();
        }
    }
    Ok(())
}

/// Two i32 on stack (a below, b on top) → one i32. `is_min`: true = min, false = max.
fn emit_i32_min_max_stack(
    ctx: &WasmCodegenContext,
    sink: &mut InstructionSink,
    is_min: bool,
) -> Result<(), GlslDiagnostics> {
    let (ta, tb) = scratch_pair(ctx)?;
    sink.local_set(tb);
    sink.local_set(ta);
    sink.local_get(ta);
    sink.local_get(tb);
    if is_min {
        sink.i32_lt_s();
    } else {
        sink.i32_gt_s();
    }
    sink.if_(BlockType::Result(ValType::I32));
    sink.local_get(ta);
    sink.else_();
    sink.local_get(tb);
    sink.end();
    Ok(())
}

/// One i32 on stack → abs(i32). Uses `ta` as single-elem scratch.
fn emit_i32_abs_from_stack(
    _ctx: &WasmCodegenContext,
    sink: &mut InstructionSink,
    ta: u32,
) -> Result<(), GlslDiagnostics> {
    sink.local_tee(ta);
    sink.i32_const(0);
    sink.i32_lt_s();
    sink.if_(BlockType::Result(ValType::I32));
    sink.i32_const(0);
    sink.local_get(ta);
    sink.i32_sub();
    sink.else_();
    sink.local_get(ta);
    sink.end();
    Ok(())
}

fn emit_mix(
    ctx: &mut WasmCodegenContext,
    sink: &mut InstructionSink,
    full_call: &Expr,
    x: &Expr,
    y: &Expr,
    a: &Expr,
    options: &WasmOptions,
) -> Result<WasmRValue, GlslDiagnostics> {
    let return_ty = infer_expr_type(ctx, full_call)?;
    let x_ty = infer_expr_type(ctx, x)?;
    let y_ty = infer_expr_type(ctx, y)?;
    let a_ty = infer_expr_type(ctx, a)?;
    if !is_float_gentype(&x_ty) || !is_float_gentype(&y_ty) {
        return Err(GlslError::new(
            ErrorCode::E0400,
            "`mix` for WASM expects float genType for x and y",
        )
        .into());
    }
    if a_ty == Type::Bool || (a_ty.is_vector() && a_ty.vector_base_type() == Some(Type::Bool)) {
        return Err(GlslError::new(
            ErrorCode::E0400,
            "`mix` bool vector overload is not supported for WASM",
        )
        .into());
    }
    if !is_float_gentype(&a_ty) {
        return Err(GlslError::new(
            ErrorCode::E0400,
            alloc::format!("`mix` third argument must be float genType, got {:?}", a_ty),
        )
        .into());
    }

    let dim = unify_two_float_gentype_dim(&x_ty, &y_ty)?;
    let a_dim = if a_ty == Type::Float || !a_ty.is_vector() {
        1u32
    } else {
        a_ty.component_count().unwrap() as u32
    };
    if a_dim != 1 && a_dim != dim {
        return Err(GlslError::new(
            ErrorCode::E0400,
            "`mix`: blend weight dimension must match x/y or be scalar",
        )
        .into());
    }

    let total = slot_span(&x_ty, dim) + slot_span(&y_ty, dim) + slot_span(&a_ty, dim);

    let numeric = WasmNumericMode::from(options.float_mode);

    match numeric {
        WasmNumericMode::Float => {
            let fb = ctx.alloc_f32(total);
            let tmp = ctx.broadcast_temp_f32.ok_or_else(|| {
                GlslDiagnostics::from(GlslError::new(
                    ErrorCode::E0400,
                    "broadcast f32 temp not allocated",
                ))
            })?;
            store_f32_arg(ctx, sink, x, &x_ty, dim, fb, options)?;
            let yb = fb + slot_span(&x_ty, dim);
            store_f32_arg(ctx, sink, y, &y_ty, dim, yb, options)?;
            let ab = yb + slot_span(&y_ty, dim);
            store_f32_arg(ctx, sink, a, &a_ty, dim, ab, options)?;
            for k in 0..dim {
                sink.local_get(yb + lhs_offset(&y_ty, k));
                sink.local_get(fb + lhs_offset(&x_ty, k));
                expr::emit_binary_op(
                    ctx,
                    sink,
                    &BinaryOp::Sub,
                    &Type::Float,
                    &Type::Float,
                    numeric,
                )?;
                sink.local_get(ab + lhs_offset(&a_ty, k));
                expr::emit_binary_op(
                    ctx,
                    sink,
                    &BinaryOp::Mult,
                    &Type::Float,
                    &Type::Float,
                    numeric,
                )?;
                sink.local_set(tmp);
                sink.local_get(fb + lhs_offset(&x_ty, k));
                sink.local_get(tmp);
                expr::emit_binary_op(
                    ctx,
                    sink,
                    &BinaryOp::Add,
                    &Type::Float,
                    &Type::Float,
                    numeric,
                )?;
            }
        }
        WasmNumericMode::Q32 => {
            let (ta, _) = scratch_pair(ctx)?;
            let ib = ctx.alloc_i32(total);
            store_q32_float_arg(ctx, sink, x, &x_ty, dim, ib, options)?;
            let yb = ib + slot_span(&x_ty, dim);
            store_q32_float_arg(ctx, sink, y, &y_ty, dim, yb, options)?;
            let ab = yb + slot_span(&y_ty, dim);
            store_q32_float_arg(ctx, sink, a, &a_ty, dim, ab, options)?;
            for k in 0..dim {
                sink.local_get(yb + lhs_offset(&y_ty, k));
                sink.local_get(ib + lhs_offset(&x_ty, k));
                expr::emit_binary_op(
                    ctx,
                    sink,
                    &BinaryOp::Sub,
                    &Type::Float,
                    &Type::Float,
                    numeric,
                )?;
                sink.local_get(ab + lhs_offset(&a_ty, k));
                expr::emit_binary_op(
                    ctx,
                    sink,
                    &BinaryOp::Mult,
                    &Type::Float,
                    &Type::Float,
                    numeric,
                )?;
                sink.local_set(ta);
                sink.local_get(ib + lhs_offset(&x_ty, k));
                sink.local_get(ta);
                expr::emit_binary_op(
                    ctx,
                    sink,
                    &BinaryOp::Add,
                    &Type::Float,
                    &Type::Float,
                    numeric,
                )?;
            }
        }
    }

    Ok(WasmRValue::from_type(return_ty))
}

fn emit_step(
    ctx: &mut WasmCodegenContext,
    sink: &mut InstructionSink,
    full_call: &Expr,
    edge: &Expr,
    x: &Expr,
    options: &WasmOptions,
) -> Result<WasmRValue, GlslDiagnostics> {
    let return_ty = infer_expr_type(ctx, full_call)?;
    let e_ty = infer_expr_type(ctx, edge)?;
    let x_ty = infer_expr_type(ctx, x)?;
    if !is_float_gentype(&x_ty) {
        return Err(GlslError::new(
            ErrorCode::E0400,
            "`step` for WASM expects float genType `x`",
        )
        .into());
    }
    if !is_float_gentype(&e_ty) && e_ty != Type::Float {
        return Err(GlslError::new(
            ErrorCode::E0400,
            alloc::format!("`step` edge must be float genType, got {:?}", e_ty),
        )
        .into());
    }

    let dim = if e_ty == Type::Float && x_ty.is_vector() {
        x_ty.component_count().unwrap() as u32
    } else {
        unify_two_float_gentype_dim(&e_ty, &x_ty)?
    };

    let total = slot_span(&e_ty, dim) + slot_span(&x_ty, dim);

    let numeric = WasmNumericMode::from(options.float_mode);
    match numeric {
        WasmNumericMode::Float => {
            let fb = ctx.alloc_f32(total);
            store_f32_arg(ctx, sink, edge, &e_ty, dim, fb, options)?;
            let xb = fb + slot_span(&e_ty, dim);
            store_f32_arg(ctx, sink, x, &x_ty, dim, xb, options)?;
            for k in 0..dim {
                sink.local_get(xb + lhs_offset(&x_ty, k));
                sink.local_get(fb + lhs_offset(&e_ty, k));
                sink.f32_lt();
                sink.if_(BlockType::Result(ValType::F32));
                sink.f32_const(0.0f32.into());
                sink.else_();
                sink.f32_const(1.0f32.into());
                sink.end();
            }
        }
        WasmNumericMode::Q32 => {
            let ib = ctx.alloc_i32(total);
            store_q32_float_arg(ctx, sink, edge, &e_ty, dim, ib, options)?;
            let xb = ib + slot_span(&e_ty, dim);
            store_q32_float_arg(ctx, sink, x, &x_ty, dim, xb, options)?;
            for k in 0..dim {
                sink.local_get(xb + lhs_offset(&x_ty, k));
                sink.local_get(ib + lhs_offset(&e_ty, k));
                sink.i32_lt_s();
                sink.if_(BlockType::Result(ValType::I32));
                sink.i32_const(0);
                sink.else_();
                sink.i32_const(Q32_ONE);
                sink.end();
            }
        }
    }

    Ok(WasmRValue::from_type(return_ty))
}

fn emit_sign(
    ctx: &mut WasmCodegenContext,
    sink: &mut InstructionSink,
    full_call: &Expr,
    arg: &Expr,
    options: &WasmOptions,
) -> Result<WasmRValue, GlslDiagnostics> {
    let return_ty = infer_expr_type(ctx, full_call)?;
    let arg_ty = infer_expr_type(ctx, arg)?;
    if !is_float_gentype(&arg_ty) {
        return Err(GlslError::new(
            ErrorCode::E0400,
            "`sign` for WASM inline supports float genType only",
        )
        .into());
    }

    let dim = if arg_ty.is_vector() {
        arg_ty.component_count().unwrap() as u32
    } else {
        1
    };

    let numeric = WasmNumericMode::from(options.float_mode);
    match numeric {
        WasmNumericMode::Float => {
            if dim > 4 {
                return Err(GlslError::new(ErrorCode::E0400, "`sign` dim too large").into());
            }
            let fb = ctx.alloc_f32(dim);
            store_f32_arg(ctx, sink, arg, &arg_ty, dim, fb, options)?;
            for k in 0..dim {
                sink.local_get(fb + lhs_offset(&arg_ty, k));
                sink.f32_const(0.0f32.into());
                sink.f32_gt();
                sink.if_(BlockType::Result(ValType::F32));
                sink.f32_const(1.0f32.into());
                sink.else_();
                sink.local_get(fb + lhs_offset(&arg_ty, k));
                sink.f32_const(0.0f32.into());
                sink.f32_lt();
                sink.if_(BlockType::Result(ValType::F32));
                sink.f32_const((-1.0f32).into());
                sink.else_();
                sink.f32_const(0.0f32.into());
                sink.end();
                sink.end();
            }
        }
        WasmNumericMode::Q32 => {
            if dim > 4 {
                return Err(GlslError::new(ErrorCode::E0400, "`sign` dim too large").into());
            }
            let ib = ctx.alloc_i32(dim);
            store_q32_float_arg(ctx, sink, arg, &arg_ty, dim, ib, options)?;
            for k in 0..dim {
                sink.local_get(ib + lhs_offset(&arg_ty, k));
                sink.i32_const(0);
                sink.i32_gt_s();
                sink.if_(BlockType::Result(ValType::I32));
                sink.i32_const(Q32_ONE);
                sink.else_();
                sink.local_get(ib + lhs_offset(&arg_ty, k));
                sink.i32_const(0);
                sink.i32_lt_s();
                sink.if_(BlockType::Result(ValType::I32));
                sink.i32_const(-Q32_ONE);
                sink.else_();
                sink.i32_const(0);
                sink.end();
                sink.end();
            }
        }
    }

    Ok(WasmRValue::from_type(return_ty))
}

fn emit_floor_q32_on_stack(sink: &mut InstructionSink) {
    sink.i32_const(16);
    sink.i32_shr_s();
    sink.i32_const(16);
    sink.i32_shl();
}

fn emit_floor(
    ctx: &mut WasmCodegenContext,
    sink: &mut InstructionSink,
    full_call: &Expr,
    arg: &Expr,
    options: &WasmOptions,
) -> Result<WasmRValue, GlslDiagnostics> {
    let return_ty = infer_expr_type(ctx, full_call)?;
    let arg_ty = infer_expr_type(ctx, arg)?;
    if !is_float_gentype(&arg_ty) {
        return Err(GlslError::new(
            ErrorCode::E0400,
            alloc::format!("`floor` for WASM expects float genType, got {:?}", arg_ty),
        )
        .into());
    }

    let dim = if arg_ty.is_vector() {
        arg_ty.component_count().unwrap() as u32
    } else {
        1
    };

    let numeric = WasmNumericMode::from(options.float_mode);
    match numeric {
        WasmNumericMode::Float => {
            expr::emit_rvalue(ctx, sink, arg, options)?;
            for _ in 0..dim {
                sink.f32_floor();
            }
        }
        WasmNumericMode::Q32 => {
            if dim > 4 {
                return Err(GlslError::new(ErrorCode::E0400, "`floor` dim too large").into());
            }
            let ib = ctx.alloc_i32(dim);
            store_q32_float_arg(ctx, sink, arg, &arg_ty, dim, ib, options)?;
            for k in 0..dim {
                sink.local_get(ib + lhs_offset(&arg_ty, k));
                emit_floor_q32_on_stack(sink);
            }
        }
    }

    Ok(WasmRValue::from_type(return_ty))
}

fn emit_fract(
    ctx: &mut WasmCodegenContext,
    sink: &mut InstructionSink,
    full_call: &Expr,
    arg: &Expr,
    options: &WasmOptions,
) -> Result<WasmRValue, GlslDiagnostics> {
    let return_ty = infer_expr_type(ctx, full_call)?;
    let arg_ty = infer_expr_type(ctx, arg)?;
    if !is_float_gentype(&arg_ty) {
        return Err(GlslError::new(
            ErrorCode::E0400,
            alloc::format!("`fract` expects float genType, got {:?}", arg_ty),
        )
        .into());
    }

    let dim = if arg_ty.is_vector() {
        arg_ty.component_count().unwrap() as u32
    } else {
        1
    };

    let numeric = WasmNumericMode::from(options.float_mode);
    match numeric {
        WasmNumericMode::Float => {
            let fb = ctx.alloc_f32(dim + 1);
            let tee_slot = fb + dim;

            store_f32_arg(ctx, sink, arg, &arg_ty, dim, fb, options)?;
            for k in 0..dim {
                sink.local_get(fb + lhs_offset(&arg_ty, k));
                sink.local_tee(tee_slot);
                sink.local_get(tee_slot);
                sink.f32_floor();
                sink.f32_sub();
            }
        }
        WasmNumericMode::Q32 => {
            if dim > 4 {
                return Err(GlslError::new(ErrorCode::E0400, "`fract` dim too large").into());
            }
            let ib = ctx.alloc_i32(dim);
            store_q32_float_arg(ctx, sink, arg, &arg_ty, dim, ib, options)?;
            for k in 0..dim {
                sink.local_get(ib + lhs_offset(&arg_ty, k));
                sink.local_get(ib + lhs_offset(&arg_ty, k));
                sink.i32_const(16);
                sink.i32_shr_s();
                sink.i32_const(16);
                sink.i32_shl();
                sink.i32_sub();
            }
        }
    }

    Ok(WasmRValue::from_type(return_ty))
}

fn emit_mod(
    ctx: &mut WasmCodegenContext,
    sink: &mut InstructionSink,
    full_call: &Expr,
    x: &Expr,
    y: &Expr,
    options: &WasmOptions,
) -> Result<WasmRValue, GlslDiagnostics> {
    let return_ty = infer_expr_type(ctx, full_call)?;
    let x_ty = infer_expr_type(ctx, x)?;
    let y_ty = infer_expr_type(ctx, y)?;
    if !is_float_gentype(&x_ty) || !is_float_gentype(&y_ty) {
        return Err(GlslError::new(
            ErrorCode::E0400,
            "`mod` for WASM expects float genType operands",
        )
        .into());
    }

    let dim = unify_two_float_gentype_dim(&x_ty, &y_ty)?;
    let total = slot_span(&x_ty, dim) + slot_span(&y_ty, dim);

    let numeric = WasmNumericMode::from(options.float_mode);
    match numeric {
        WasmNumericMode::Float => {
            let fb = ctx.alloc_f32(total);
            let tmp = ctx.broadcast_temp_f32.ok_or_else(|| {
                GlslDiagnostics::from(GlslError::new(
                    ErrorCode::E0400,
                    "broadcast f32 temp not allocated",
                ))
            })?;
            store_f32_arg(ctx, sink, x, &x_ty, dim, fb, options)?;
            let yb = fb + slot_span(&x_ty, dim);
            store_f32_arg(ctx, sink, y, &y_ty, dim, yb, options)?;
            for k in 0..dim {
                sink.local_get(fb + lhs_offset(&x_ty, k));
                sink.local_get(yb + lhs_offset(&y_ty, k));
                sink.f32_div();
                sink.local_set(tmp);
                sink.local_get(fb + lhs_offset(&x_ty, k));
                sink.local_get(yb + lhs_offset(&y_ty, k));
                sink.local_get(tmp);
                sink.f32_floor();
                sink.f32_mul();
                sink.f32_sub();
            }
        }
        WasmNumericMode::Q32 => {
            let ib = ctx.alloc_i32(total);
            let (ta, _) = scratch_pair(ctx)?;
            let bt = ctx.broadcast_temp_i32.ok_or_else(|| {
                GlslDiagnostics::from(GlslError::new(
                    ErrorCode::E0400,
                    "broadcast i32 temp not allocated",
                ))
            })?;
            store_q32_float_arg(ctx, sink, x, &x_ty, dim, ib, options)?;
            let yb = ib + slot_span(&x_ty, dim);
            store_q32_float_arg(ctx, sink, y, &y_ty, dim, yb, options)?;
            for k in 0..dim {
                sink.local_get(ib + lhs_offset(&x_ty, k));
                sink.local_get(yb + lhs_offset(&y_ty, k));
                expr::emit_binary_op(
                    ctx,
                    sink,
                    &BinaryOp::Div,
                    &Type::Float,
                    &Type::Float,
                    numeric,
                )?;
                sink.local_set(bt);
                sink.local_get(bt);
                sink.i32_const(16);
                sink.i32_shr_s();
                sink.i32_const(16);
                sink.i32_shl();
                sink.local_set(ta);
                sink.local_get(ib + lhs_offset(&x_ty, k));
                sink.local_get(yb + lhs_offset(&y_ty, k));
                sink.local_get(ta);
                expr::emit_binary_op(
                    ctx,
                    sink,
                    &BinaryOp::Mult,
                    &Type::Float,
                    &Type::Float,
                    numeric,
                )?;
                expr::emit_binary_op(
                    ctx,
                    sink,
                    &BinaryOp::Sub,
                    &Type::Float,
                    &Type::Float,
                    numeric,
                )?;
            }
        }
    }

    Ok(WasmRValue::from_type(return_ty))
}

fn emit_smoothstep(
    ctx: &mut WasmCodegenContext,
    sink: &mut InstructionSink,
    full_call: &Expr,
    edge0: &Expr,
    edge1: &Expr,
    x: &Expr,
    options: &WasmOptions,
) -> Result<WasmRValue, GlslDiagnostics> {
    let return_ty = infer_expr_type(ctx, full_call)?;
    let e0_ty = infer_expr_type(ctx, edge0)?;
    let e1_ty = infer_expr_type(ctx, edge1)?;
    let x_ty = infer_expr_type(ctx, x)?;
    if !is_float_gentype(&e0_ty) || !is_float_gentype(&e1_ty) || !is_float_gentype(&x_ty) {
        return Err(GlslError::new(
            ErrorCode::E0400,
            "`smoothstep` for WASM expects float genType for all arguments",
        )
        .into());
    }

    let dim = unify_three_float_gentype_dim(&e0_ty, &e1_ty, &x_ty)?;
    let e0b_base = 0u32;
    let e1b_base = slot_span(&e0_ty, dim);
    let xb_base = e1b_base + slot_span(&e1_ty, dim);
    let temps_after = xb_base + slot_span(&x_ty, dim);
    let scratch_t = temps_after;
    let scratch_tt = scratch_t + 1;
    let layout_slots = temps_after + 2;

    let numeric = WasmNumericMode::from(options.float_mode);
    match numeric {
        WasmNumericMode::Float => {
            let fb = ctx.alloc_f32(layout_slots);
            store_f32_arg(ctx, sink, edge0, &e0_ty, dim, fb + e0b_base, options)?;
            store_f32_arg(ctx, sink, edge1, &e1_ty, dim, fb + e1b_base, options)?;
            store_f32_arg(ctx, sink, x, &x_ty, dim, fb + xb_base, options)?;
            for k in 0..dim {
                sink.local_get(fb + e1b_base + lhs_offset(&e1_ty, k));
                sink.local_get(fb + e0b_base + lhs_offset(&e0_ty, k));
                sink.f32_sub();
                sink.local_set(fb + scratch_t);
                sink.local_get(fb + xb_base + lhs_offset(&x_ty, k));
                sink.local_get(fb + e0b_base + lhs_offset(&e0_ty, k));
                sink.f32_sub();
                sink.local_get(fb + scratch_t);
                sink.f32_div();
                sink.f32_const(0.0f32.into());
                sink.f32_max();
                sink.f32_const(1.0f32.into());
                sink.f32_min();
                sink.local_set(fb + scratch_t);
                sink.local_get(fb + scratch_t);
                sink.local_get(fb + scratch_t);
                sink.f32_mul();
                sink.local_set(fb + scratch_tt);
                sink.f32_const(3.0f32.into());
                sink.local_get(fb + scratch_t);
                sink.f32_const(2.0f32.into());
                sink.f32_mul();
                sink.f32_sub();
                sink.local_get(fb + scratch_tt);
                sink.f32_mul();
            }
        }
        WasmNumericMode::Q32 => {
            let ib = ctx.alloc_i32(layout_slots);
            let (ta, tb) = scratch_pair(ctx)?;
            let bt = ctx.broadcast_temp_i32.ok_or_else(|| {
                GlslDiagnostics::from(GlslError::new(
                    ErrorCode::E0400,
                    "broadcast i32 temp not allocated",
                ))
            })?;
            store_q32_float_arg(ctx, sink, edge0, &e0_ty, dim, ib + e0b_base, options)?;
            store_q32_float_arg(ctx, sink, edge1, &e1_ty, dim, ib + e1b_base, options)?;
            store_q32_float_arg(ctx, sink, x, &x_ty, dim, ib + xb_base, options)?;
            for k in 0..dim {
                sink.local_get(ib + e1b_base + lhs_offset(&e1_ty, k));
                sink.local_get(ib + e0b_base + lhs_offset(&e0_ty, k));
                expr::emit_binary_op(
                    ctx,
                    sink,
                    &BinaryOp::Sub,
                    &Type::Float,
                    &Type::Float,
                    numeric,
                )?;
                sink.local_set(bt);
                sink.local_get(ib + xb_base + lhs_offset(&x_ty, k));
                sink.local_get(ib + e0b_base + lhs_offset(&e0_ty, k));
                expr::emit_binary_op(
                    ctx,
                    sink,
                    &BinaryOp::Sub,
                    &Type::Float,
                    &Type::Float,
                    numeric,
                )?;
                sink.local_get(bt);
                expr::emit_binary_op(
                    ctx,
                    sink,
                    &BinaryOp::Div,
                    &Type::Float,
                    &Type::Float,
                    numeric,
                )?;
                sink.local_set(bt);
                sink.local_get(bt);
                sink.i32_const(0);
                emit_i32_min_max_stack(ctx, sink, false)?;
                sink.i32_const(Q32_ONE);
                emit_i32_min_max_stack(ctx, sink, true)?;
                sink.local_set(ta);
                sink.local_get(ta);
                sink.local_get(ta);
                expr::emit_binary_op(
                    ctx,
                    sink,
                    &BinaryOp::Mult,
                    &Type::Float,
                    &Type::Float,
                    numeric,
                )?;
                sink.local_set(tb);
                sink.i32_const(Q32_THREE);
                sink.local_get(ta);
                sink.i32_const(Q32_TWO);
                expr::emit_binary_op(
                    ctx,
                    sink,
                    &BinaryOp::Mult,
                    &Type::Float,
                    &Type::Float,
                    numeric,
                )?;
                expr::emit_binary_op(
                    ctx,
                    sink,
                    &BinaryOp::Sub,
                    &Type::Float,
                    &Type::Float,
                    numeric,
                )?;
                sink.local_get(tb);
                expr::emit_binary_op(
                    ctx,
                    sink,
                    &BinaryOp::Mult,
                    &Type::Float,
                    &Type::Float,
                    numeric,
                )?;
            }
        }
    }

    Ok(WasmRValue::from_type(return_ty))
}
