//! Scalar and vector type constructor code generation.

use wasm_encoder::{BlockType, InstructionSink};

use crate::codegen::context::WasmCodegenContext;
use crate::codegen::expr;
use crate::codegen::rvalue::WasmRValue;
use crate::options::WasmOptions;
use lp_glsl_frontend::error::GlslDiagnostics;
use lp_glsl_frontend::semantic::type_check::{is_scalar_type_name, is_vector_type_name};
use lp_glsl_frontend::semantic::types::Type;

/// Emit scalar type constructor: int(x), float(x), bool(x), uint(x).
pub fn emit_scalar_constructor(
    ctx: &mut WasmCodegenContext,
    sink: &mut InstructionSink,
    type_name: &str,
    args: &[glsl::syntax::Expr],
    options: &WasmOptions,
) -> Result<WasmRValue, GlslDiagnostics> {
    if !is_scalar_type_name(type_name) {
        return Err(lp_glsl_frontend::error::GlslError::new(
            lp_glsl_frontend::error::ErrorCode::E0112,
            alloc::format!("`{type_name}` is not a scalar type"),
        )
        .into());
    }

    if args.len() != 1 {
        return Err(lp_glsl_frontend::error::GlslError::new(
            lp_glsl_frontend::error::ErrorCode::E0115,
            alloc::format!("`{type_name}` constructor requires exactly one argument"),
        )
        .into());
    }

    let arg_rv = expr::emit_rvalue(ctx, sink, &args[0], options)?;
    let result_ty = match type_name {
        "bool" => Type::Bool,
        "int" => Type::Int,
        "uint" => Type::UInt,
        "float" => Type::Float,
        _ => {
            return Err(lp_glsl_frontend::error::GlslError::new(
                lp_glsl_frontend::error::ErrorCode::E0112,
                alloc::format!("`{type_name}` is not a scalar type"),
            )
            .into());
        }
    };

    emit_coercion(ctx, sink, &arg_rv.ty, &result_ty);
    Ok(WasmRValue::scalar(result_ty))
}

/// Emit vector constructor: vec2, vec3, vec4, ivec2/3/4, uvec2/3/4, bvec2/3/4.
pub fn emit_vector_constructor(
    ctx: &mut WasmCodegenContext,
    sink: &mut InstructionSink,
    type_name: &str,
    args: &[glsl::syntax::Expr],
    options: &WasmOptions,
) -> Result<WasmRValue, GlslDiagnostics> {
    if !is_vector_type_name(type_name) {
        return Err(lp_glsl_frontend::error::GlslError::new(
            lp_glsl_frontend::error::ErrorCode::E0112,
            alloc::format!("`{type_name}` is not a vector type"),
        )
        .into());
    }

    let result_ty = match type_name {
        "vec2" => Type::Vec2,
        "vec3" => Type::Vec3,
        "vec4" => Type::Vec4,
        "ivec2" => Type::IVec2,
        "ivec3" => Type::IVec3,
        "ivec4" => Type::IVec4,
        "uvec2" => Type::UVec2,
        "uvec3" => Type::UVec3,
        "uvec4" => Type::UVec4,
        "bvec2" => Type::BVec2,
        "bvec3" => Type::BVec3,
        "bvec4" => Type::BVec4,
        _ => {
            return Err(lp_glsl_frontend::error::GlslError::new(
                lp_glsl_frontend::error::ErrorCode::E0112,
                alloc::format!("`{type_name}` is not a vector type"),
            )
            .into());
        }
    };
    let component_count = result_ty.component_count().unwrap();
    let base_ty = result_ty.vector_base_type().unwrap();

    if args.len() == 1 {
        let arg_rv = expr::emit_rvalue(ctx, sink, &args[0], options)?;
        let arg_count = if arg_rv.ty.is_scalar() {
            1
        } else {
            arg_rv.ty.component_count().unwrap_or(1)
        };
        if arg_count == 1 {
            // Broadcast: replicate scalar to all components (with coercion per component)
            let temp_idx = ctx.get_broadcast_temp(arg_rv.ty.clone());
            sink.local_tee(temp_idx);
            for _ in 0..component_count {
                sink.local_get(temp_idx);
                emit_coercion(ctx, sink, &arg_rv.ty, &base_ty);
            }
        } else {
            // Vector source: spill all source components, then take the first `component_count`
            // (shortening) or load all source components and pad with defaults (lengthening).
            let src_base = arg_rv.ty.vector_base_type().unwrap();
            let slots = arg_count.max(component_count);
            assert!(slots <= 4, "vector constructor temp overflow");
            let temp_base = ctx.vector_conv_temp(&src_base, slots);
            for i in (0..arg_count).rev() {
                sink.local_set(temp_base + i as u32);
            }
            for i in 0..component_count {
                if i < arg_count {
                    sink.local_get(temp_base + i as u32);
                    emit_coercion(ctx, sink, &src_base, &base_ty);
                } else {
                    emit_default_vector_component(ctx, sink, &base_ty);
                }
            }
        }
    } else {
        // Multiple args: emit each, coerce each value immediately
        for arg in args.iter() {
            let arg_rv = expr::emit_rvalue(ctx, sink, arg, options)?;
            let from_ty = if arg_rv.ty.is_scalar() {
                arg_rv.ty.clone()
            } else {
                arg_rv.ty.vector_base_type().unwrap()
            };
            for _ in 0..arg_rv.stack_count {
                emit_coercion(ctx, sink, &from_ty, &base_ty);
            }
        }
    }
    Ok(WasmRValue::from_type(result_ty))
}

/// GLSL default for missing vector components: 0 / 0.0 / false.
fn emit_default_vector_component(
    ctx: &WasmCodegenContext,
    sink: &mut InstructionSink,
    base_ty: &Type,
) {
    use crate::codegen::numeric::WasmNumericMode;
    match base_ty {
        Type::Float => {
            if ctx.numeric == WasmNumericMode::Q32 {
                sink.i32_const(0);
            } else {
                sink.f32_const(0.0f32.into());
            }
        }
        Type::Int | Type::UInt | Type::Bool => {
            sink.i32_const(0);
        }
        _ => panic!("emit_default_vector_component: {:?}", base_ty),
    }
}

fn emit_coercion(ctx: &WasmCodegenContext, sink: &mut InstructionSink, from: &Type, to: &Type) {
    if from == to {
        return;
    }

    let numeric = ctx.numeric;

    match (from, to) {
        (Type::Int, Type::Float) => {
            if numeric == crate::codegen::numeric::WasmNumericMode::Q32 {
                let temp = ctx
                    .binary_op_i32_base
                    .expect("binary_op temps not allocated");
                sink.local_tee(temp);
                sink.i32_const(-32768);
                sink.i32_lt_s();
                sink.if_(BlockType::Result(wasm_encoder::ValType::I32));
                sink.i32_const(-32768);
                sink.else_();
                sink.local_get(temp);
                sink.end();
                sink.local_tee(temp);
                sink.i32_const(32767);
                sink.i32_gt_s();
                sink.if_(BlockType::Result(wasm_encoder::ValType::I32));
                sink.i32_const(32767);
                sink.else_();
                sink.local_get(temp);
                sink.end();
                sink.i32_const(16);
                sink.i32_shl();
            } else {
                sink.f32_convert_i32_s();
            }
        }
        (Type::UInt, Type::Float) => {
            if numeric == crate::codegen::numeric::WasmNumericMode::Q32 {
                let temp = ctx
                    .binary_op_i32_base
                    .expect("binary_op temps not allocated");
                sink.local_tee(temp);
                sink.i32_const(32767);
                sink.i32_gt_u();
                sink.if_(BlockType::Result(wasm_encoder::ValType::I32));
                sink.i32_const(32767);
                sink.else_();
                sink.local_get(temp);
                sink.end();
                sink.i32_const(16);
                sink.i32_shl();
            } else {
                sink.f32_convert_i32_s();
            }
        }
        (Type::Bool, Type::Float) => {
            if numeric == crate::codegen::numeric::WasmNumericMode::Q32 {
                sink.i32_const(16);
                sink.i32_shl();
            } else {
                sink.f32_convert_i32_s();
            }
        }
        (Type::Float, Type::Int) => {
            if numeric == crate::codegen::numeric::WasmNumericMode::Q32 {
                let temp = ctx
                    .binary_op_i32_base
                    .expect("binary_op temps not allocated");
                sink.local_tee(temp);
                sink.i32_const(0);
                sink.i32_lt_s();
                sink.if_(BlockType::Result(wasm_encoder::ValType::I32));
                sink.local_get(temp);
                sink.i32_const((1 << 16) - 1);
                sink.i32_add();
                sink.i32_const(16);
                sink.i32_shr_s();
                sink.else_();
                sink.local_get(temp);
                sink.i32_const(16);
                sink.i32_shr_s();
                sink.end();
            } else {
                sink.i32_trunc_f32_s();
            }
        }
        (Type::Float, Type::UInt) => {
            if numeric == crate::codegen::numeric::WasmNumericMode::Q32 {
                let temp = ctx
                    .binary_op_i32_base
                    .expect("binary_op temps not allocated");
                sink.local_tee(temp);
                sink.i32_const(0);
                sink.i32_lt_s();
                sink.if_(BlockType::Result(wasm_encoder::ValType::I32));
                sink.local_get(temp);
                sink.i32_const((1 << 16) - 1);
                sink.i32_add();
                sink.i32_const(16);
                sink.i32_shr_s();
                sink.else_();
                sink.local_get(temp);
                sink.i32_const(16);
                sink.i32_shr_s();
                sink.end();
            } else {
                sink.i32_trunc_f32_u();
            }
        }
        (_, Type::Bool) => {
            sink.i32_const(0);
            sink.i32_ne();
        }
        _ => {}
    }
}
