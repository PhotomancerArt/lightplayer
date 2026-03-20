//! LPFX (`lpfx_*`) host calls: flattened `In` args, `i32` pointers into linear memory, post-call loads.

use glsl::syntax::Expr;
use lp_glsl_frontend::error::{ErrorCode, GlslDiagnostics, GlslError};
use lp_glsl_frontend::semantic::functions::ParamQualifier;
use lp_glsl_frontend::semantic::lpfx::lpfx_fn::LpfxFnImpl;
use lp_glsl_frontend::semantic::lpfx::lpfx_fn_registry::find_lpfx_fn;
use lp_glsl_frontend::semantic::types::Type;
use wasm_encoder::{InstructionSink, MemArg};

use crate::codegen::builtin_wasm_import_types::wasm_import_val_types;
use crate::codegen::context::WasmCodegenContext;
use crate::codegen::expr;
use crate::codegen::expr::infer_expr_type;
use crate::codegen::memory::{LPFX_OUT_PARAM_BASE, LPFX_SCRATCH_BYTES};
use crate::codegen::rvalue::WasmRValue;
use crate::options::WasmOptions;

fn memarg_i32_natural() -> MemArg {
    MemArg {
        offset: 0,
        align: 2,
        memory_index: 0,
    }
}

fn vector_out_bytes(ty: &Type) -> Result<u32, GlslDiagnostics> {
    let n = match ty {
        Type::Vec2 | Type::IVec2 | Type::UVec2 | Type::BVec2 => 2u32,
        Type::Vec3 | Type::IVec3 | Type::UVec3 | Type::BVec3 => 3,
        Type::Vec4 | Type::IVec4 | Type::UVec4 | Type::BVec4 => 4,
        _ => {
            return Err(GlslError::new(
                ErrorCode::E0400,
                alloc::format!("LPFX `out` parameter must be a vector type, got {:?}", ty),
            )
            .into());
        }
    };
    Ok(n * 4)
}

fn out_variable_name<'a>(arg: &'a Expr) -> Result<&'a str, GlslDiagnostics> {
    match arg {
        Expr::Variable(ident, _) => Ok(ident.name.as_str()),
        _ => Err(GlslError::new(
            ErrorCode::E0400,
            "LPFX `out` argument must be a simple variable",
        )
        .into()),
    }
}

pub fn emit_lpfx_call(
    ctx: &mut WasmCodegenContext,
    sink: &mut InstructionSink,
    full_call: &Expr,
    name: &str,
    args: &[Expr],
    options: &WasmOptions,
) -> Result<WasmRValue, GlslDiagnostics> {
    let return_ty = infer_expr_type(ctx, full_call)?;
    let arg_types: alloc::vec::Vec<Type> = args
        .iter()
        .map(|a| infer_expr_type(ctx, a))
        .collect::<Result<_, _>>()?;

    let func = find_lpfx_fn(name, &arg_types).ok_or_else(|| {
        GlslDiagnostics::from(GlslError::new(
            ErrorCode::E0400,
            alloc::format!("unknown LPFX overload `{name}` for {:?}", arg_types),
        ))
    })?;

    let builtin_id = match &func.impls {
        LpfxFnImpl::Decimal { q32_impl, .. } => *q32_impl,
        LpfxFnImpl::NonDecimal(id) => *id,
    };

    let func_idx = *ctx.builtin_func_index.get(&builtin_id).ok_or_else(|| {
        GlslDiagnostics::from(GlslError::new(
            ErrorCode::E0400,
            alloc::format!("LPFX builtin {:?} not in WASM import map", builtin_id),
        ))
    })?;

    let glsl_return = &func.glsl_sig.return_type;
    let return_is_vector = glsl_return.is_vector();

    let mut has_out = false;
    for p in func.glsl_sig.parameters.iter() {
        if matches!(p.qualifier, ParamQualifier::Out | ParamQualifier::InOut) {
            has_out = true;
            break;
        }
    }

    if return_is_vector && has_out {
        return Err(GlslError::new(
            ErrorCode::E0400,
            "LPFX calls with vector return and `out` parameters are not supported for WASM",
        )
        .into());
    }

    let (_, wasm_results) = wasm_import_val_types(builtin_id);
    let void_wasm = wasm_results.is_empty();
    if void_wasm != return_is_vector {
        return Err(GlslError::new(
            ErrorCode::E0400,
            alloc::format!(
                "internal: LPFX {:?} GLSL return vector={} vs WASM void={}",
                builtin_id,
                return_is_vector,
                void_wasm
            ),
        )
        .into());
    }

    let mut scratch_next = LPFX_OUT_PARAM_BASE;

    let result_area_off = if return_is_vector {
        let cnt = glsl_return.component_count().unwrap() as u32;
        let bytes = cnt * 4;
        if scratch_next + bytes > LPFX_OUT_PARAM_BASE + LPFX_SCRATCH_BYTES {
            return Err(GlslError::new(ErrorCode::E0400, "LPFX scratch overflow (result)").into());
        }
        let off = scratch_next;
        scratch_next += bytes;
        sink.i32_const(off as i32);
        Some(off)
    } else {
        None
    };

    let mut out_writeback: alloc::vec::Vec<(u32, alloc::string::String, u32)> =
        alloc::vec::Vec::new();

    for (param, arg) in func.glsl_sig.parameters.iter().zip(args.iter()) {
        match param.qualifier {
            ParamQualifier::In => {
                expr::emit_rvalue(ctx, sink, arg, options)?;
            }
            ParamQualifier::Out | ParamQualifier::InOut => {
                let nbytes = vector_out_bytes(&param.ty)?;
                if scratch_next + nbytes > LPFX_OUT_PARAM_BASE + LPFX_SCRATCH_BYTES {
                    return Err(
                        GlslError::new(ErrorCode::E0400, "LPFX scratch overflow (out)").into(),
                    );
                }
                let off = scratch_next;
                scratch_next += nbytes;
                sink.i32_const(off as i32);
                let vname = out_variable_name(arg)?;
                let comps = param.ty.component_count().unwrap() as u32;
                out_writeback.push((off, alloc::string::String::from(vname), comps));
            }
        }
    }

    sink.call(func_idx);

    let mem = memarg_i32_natural();

    if let Some(roff) = result_area_off {
        let cnt = glsl_return.component_count().unwrap() as u32;
        for i in 0..cnt {
            sink.i32_const((roff + i * 4) as i32);
            sink.i32_load(mem);
        }
        return Ok(WasmRValue::from_type(return_ty));
    }

    if !out_writeback.is_empty() {
        let scratch = ctx.broadcast_temp_i32.ok_or_else(|| {
            GlslDiagnostics::from(GlslError::new(
                ErrorCode::E0400,
                "broadcast i32 temp not allocated (LPFX scalar + out)",
            ))
        })?;
        sink.local_set(scratch);
        for (off, vname, comps) in &out_writeback {
            let info = ctx.lookup_local(vname.as_str()).ok_or_else(|| {
                GlslDiagnostics::from(GlslError::new(
                    ErrorCode::E0400,
                    alloc::format!("unknown variable `{}` for LPFX out", vname),
                ))
            })?;
            for c in 0..*comps {
                sink.i32_const((off + c * 4) as i32);
                sink.i32_load(mem);
                sink.local_set(info.base_index + c);
            }
        }
        sink.local_get(scratch);
    }

    Ok(WasmRValue::from_type(return_ty))
}
