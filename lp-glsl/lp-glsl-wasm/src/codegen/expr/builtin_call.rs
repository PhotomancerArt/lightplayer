//! Q32 `__lp_q32_*` WASM import calls (component-wise genType).

use wasm_encoder::InstructionSink;

use crate::codegen::builtin_wasm_import_types;
use crate::codegen::context::WasmCodegenContext;
use crate::codegen::expr;
use crate::codegen::expr::infer_expr_type;
use crate::codegen::rvalue::WasmRValue;
use crate::options::WasmOptions;
use glsl::syntax::Expr;
use lp_glsl_builtin_ids::{BuiltinId, glsl_q32_math_builtin_id};
use lp_glsl_frontend::FloatMode;
use lp_glsl_frontend::error::{ErrorCode, GlslDiagnostics, GlslError};
use lp_glsl_frontend::semantic::types::Type;

/// Emit a standard Q32 math libcall (`sin`, `mul`, `fma`, …) as WASM `call` to an import.
pub fn emit_q32_math_libcall(
    ctx: &mut WasmCodegenContext,
    sink: &mut InstructionSink,
    full_call: &Expr,
    name: &str,
    args: &[Expr],
    options: &WasmOptions,
) -> Result<WasmRValue, GlslDiagnostics> {
    if options.float_mode != FloatMode::Q32 {
        return Err(GlslError::new(
            ErrorCode::E0400,
            alloc::format!("built-in `{name}` as import requires Q32 float mode"),
        )
        .into());
    }

    let Some(id) = glsl_q32_math_builtin_id(name, args.len()) else {
        return Err(GlslError::new(
            ErrorCode::E0400,
            alloc::format!("built-in `{name}` is not available as a Q32 WASM import"),
        )
        .into());
    };

    let Some(&func_idx) = ctx.builtin_func_index.get(&id) else {
        return Err(GlslError::new(
            ErrorCode::E0400,
            alloc::format!("internal: missing WASM import index for {:?}", id),
        )
        .into());
    };

    let (wasm_params, wasm_results) = builtin_wasm_import_types::wasm_import_val_types(id);
    if !wasm_results.is_empty() && wasm_results.len() != 1 {
        return Err(GlslError::new(
            ErrorCode::E0400,
            alloc::format!(
                "WASM codegen: multi-value builtin returns not supported ({:?})",
                id
            ),
        )
        .into());
    }
    if wasm_params.is_empty() && !args.is_empty() {
        return Err(GlslError::new(
            ErrorCode::E0400,
            alloc::format!("internal: wasm params empty for {:?}", id),
        )
        .into());
    }

    let return_ty = infer_expr_type(ctx, full_call)?;
    let arg_tys: alloc::vec::Vec<Type> = args
        .iter()
        .map(|a| infer_expr_type(ctx, a))
        .collect::<Result<_, _>>()?;

    if id == BuiltinId::LpQ32Ldexp {
        let dim = float_gentype_dim_only("ldexp", &arg_tys[0])?;
        let base = ctx.alloc_i32(dim + 1);
        return emit_q32_ldexp(
            ctx, sink, return_ty, args, &arg_tys, func_idx, base, options,
        );
    }

    let dim = unify_float_gentype_dim(name, &arg_tys)?;
    let ast_arity = args.len();
    let wasm_arity = wasm_params.len();
    if ast_arity != wasm_arity {
        return Err(GlslError::new(
            ErrorCode::E0400,
            alloc::format!(
                "internal: AST arity {} != wasm arity {} for {:?}",
                ast_arity,
                wasm_arity,
                id
            ),
        )
        .into());
    }

    let total_slots = (wasm_arity as u32).saturating_mul(dim);
    let base = ctx.alloc_i32(total_slots);

    store_flattened_q32_args(ctx, sink, args, &arg_tys, dim, base, options)?;

    let d = dim as usize;
    for k in 0..d {
        for j in 0..wasm_arity {
            sink.local_get(base + (j as u32) * dim + k as u32);
        }
        sink.call(func_idx);
    }

    Ok(WasmRValue::from_type(return_ty))
}

/// `ldexp(genType, int)` — exponent is a single int shared by all components.
fn emit_q32_ldexp(
    ctx: &mut WasmCodegenContext,
    sink: &mut InstructionSink,
    return_ty: Type,
    args: &[Expr],
    arg_tys: &[Type],
    func_idx: u32,
    base: u32,
    options: &WasmOptions,
) -> Result<WasmRValue, GlslDiagnostics> {
    if args.len() != 2 || arg_tys.len() != 2 {
        return Err(
            GlslError::new(ErrorCode::E0400, "internal: ldexp expects two arguments").into(),
        );
    }
    if arg_tys[1] != Type::Int {
        return Err(GlslError::new(
            ErrorCode::E0400,
            alloc::format!("`ldexp` second argument must be int, got {:?}", arg_tys[1]),
        )
        .into());
    }

    let dim = float_gentype_dim_only("ldexp", &arg_tys[0])?;

    let t0 = &arg_tys[0];
    let slot_f = base;
    if t0.is_vector() {
        expr::emit_rvalue(ctx, sink, &args[0], options)?;
        for i in (0..dim as usize).rev() {
            sink.local_set(slot_f + i as u32);
        }
    } else {
        expr::emit_rvalue(ctx, sink, &args[0], options)?;
        sink.local_set(slot_f);
        for k in 1..dim as usize {
            sink.local_get(slot_f);
            sink.local_set(slot_f + k as u32);
        }
    }

    expr::emit_rvalue(ctx, sink, &args[1], options)?;
    sink.local_set(base + dim);

    for k in 0..dim as usize {
        sink.local_get(base + k as u32);
        sink.local_get(base + dim);
        sink.call(func_idx);
    }

    Ok(WasmRValue::from_type(return_ty))
}

fn float_gentype_dim_only(name: &str, t: &Type) -> Result<u32, GlslDiagnostics> {
    match t {
        Type::Float => Ok(1),
        Type::Vec2 => Ok(2),
        Type::Vec3 => Ok(3),
        Type::Vec4 => Ok(4),
        _ => Err(GlslError::new(
            ErrorCode::E0400,
            alloc::format!("built-in `{name}`: expected float genType, got {:?}", t),
        )
        .into()),
    }
}

fn unify_float_gentype_dim(name: &str, arg_tys: &[Type]) -> Result<u32, GlslDiagnostics> {
    let mut dim: u32 = 1;
    for t in arg_tys {
        match t {
            Type::Float => {}
            Type::Vec2 | Type::Vec3 | Type::Vec4 => {
                let Some(n) = t.component_count() else {
                    return Err(GlslError::new(
                        ErrorCode::E0400,
                        alloc::format!("built-in `{name}`: bad vector type"),
                    )
                    .into());
                };
                let n = n as u32;
                if t.vector_base_type() != Some(Type::Float) {
                    return Err(GlslError::new(
                        ErrorCode::E0400,
                        alloc::format!("built-in `{name}`: expected float vector, got {:?}", t),
                    )
                    .into());
                }
                dim = dim.max(n);
            }
            _ => {
                return Err(GlslError::new(
                    ErrorCode::E0400,
                    alloc::format!(
                        "built-in `{name}` for WASM expects float genType, got {:?}",
                        t
                    ),
                )
                .into());
            }
        }
    }

    for t in arg_tys {
        match t {
            Type::Float => {}
            _ if t.is_vector() => {
                let n = t.component_count().unwrap() as u32;
                if n != dim {
                    return Err(GlslError::new(
                        ErrorCode::E0400,
                        alloc::format!(
                            "built-in `{name}`: mismatched vector sizes (expected dim {}, got {})",
                            dim,
                            n
                        ),
                    )
                    .into());
                }
            }
            _ => {
                return Err(GlslError::new(
                    ErrorCode::E0400,
                    alloc::format!("built-in `{name}`: invalid argument type {:?}", t),
                )
                .into());
            }
        }
    }

    Ok(dim)
}

fn store_flattened_q32_args(
    ctx: &mut WasmCodegenContext,
    sink: &mut InstructionSink,
    args: &[Expr],
    arg_tys: &[Type],
    dim: u32,
    base: u32,
    options: &WasmOptions,
) -> Result<(), GlslDiagnostics> {
    let d = dim as usize;
    for (j, arg) in args.iter().enumerate() {
        let ty = &arg_tys[j];
        let slot_start = base + (j as u32) * dim;
        if ty.is_vector() {
            expr::emit_rvalue(ctx, sink, arg, options)?;
            for i in (0..d).rev() {
                sink.local_set(slot_start + i as u32);
            }
        } else {
            expr::emit_rvalue(ctx, sink, arg, options)?;
            sink.local_set(slot_start);
            for k in 1..d {
                sink.local_get(slot_start);
                sink.local_set(slot_start + k as u32);
            }
        }
    }
    Ok(())
}
