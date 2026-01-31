use crate::error::{ErrorCode, GlslError, source_span_to_location};
use crate::frontend::codegen::context::CodegenContext;
use crate::semantic::types::Type as GlslType;
use cranelift_codegen::ir::{
    InstBuilder, Value,
    condcodes::{FloatCC, IntCC},
    types,
};

use alloc::format;

pub fn coerce_to_type<M: cranelift_module::Module>(
    ctx: &mut CodegenContext<'_, M>,
    val: Value,
    from_ty: &GlslType,
    to_ty: &GlslType,
) -> Result<Value, GlslError> {
    coerce_to_type_with_location(ctx, val, from_ty, to_ty, None)
}

pub fn coerce_to_type_with_location<M: cranelift_module::Module>(
    ctx: &mut CodegenContext<'_, M>,
    val: Value,
    from_ty: &GlslType,
    to_ty: &GlslType,
    span: Option<glsl::syntax::SourceSpan>,
) -> Result<Value, GlslError> {
    if from_ty == to_ty {
        return Ok(val);
    }

    crate::debug!("coerce_to_type: {:?} -> {:?}", from_ty, to_ty);
    match (from_ty, to_ty) {
        (GlslType::Int, GlslType::Float) => {
            // int → float: fcvt_from_sint
            Ok(ctx.builder.ins().fcvt_from_sint(types::F32, val))
        }
        // Boolean to numeric conversions
        (GlslType::Bool, GlslType::Int) => {
            // bool → int: false → 0, true → 1
            // val is i8 (0 or 1), extend to i32
            Ok(ctx.builder.ins().uextend(types::I32, val))
        }
        (GlslType::Bool, GlslType::Float) => {
            // bool → float: false → 0.0, true → 1.0
            // val is i8 (0 or 1), convert to i32 then to float
            let i32_val = ctx.builder.ins().uextend(types::I32, val);
            let f32_val = ctx.builder.ins().fcvt_from_sint(types::F32, i32_val);
            Ok(f32_val)
        }
        // Boolean to uint conversion
        (GlslType::Bool, GlslType::UInt) => {
            // bool → uint: false → 0u, true → 1u
            // val is i8 (0 or 1), extend to i32 (treat as unsigned)
            Ok(ctx.builder.ins().uextend(types::I32, val))
        }
        // Numeric to boolean conversions
        (GlslType::Int, GlslType::Bool) => {
            // int → bool: 0 → false, non-zero → true
            // val is i32, compare with 0, convert result to i8
            let zero = ctx.builder.ins().iconst(types::I32, 0);
            let cmp = ctx.builder.ins().icmp(IntCC::NotEqual, val, zero);
            let one = ctx.builder.ins().iconst(types::I8, 1);
            let zero_i8 = ctx.builder.ins().iconst(types::I8, 0);
            Ok(ctx.builder.ins().select(cmp, one, zero_i8))
        }
        (GlslType::Float, GlslType::Bool) => {
            // float → bool: 0.0 → false, non-zero → true
            // val is f32, compare with 0.0, convert result to i8
            let zero = ctx.builder.ins().f32const(0.0);
            let cmp = ctx.builder.ins().fcmp(FloatCC::NotEqual, val, zero);
            let one = ctx.builder.ins().iconst(types::I8, 1);
            let zero_i8 = ctx.builder.ins().iconst(types::I8, 0);
            Ok(ctx.builder.ins().select(cmp, one, zero_i8))
        }
        (GlslType::Float, GlslType::Int) => {
            // float → int: truncates fractional part toward zero
            // val is f32, convert to i32 using fcvt_to_sint
            Ok(ctx.builder.ins().fcvt_to_sint(types::I32, val))
        }
        // uint to boolean conversion
        (GlslType::UInt, GlslType::Bool) => {
            // uint → bool: 0 → false, non-zero → true
            // val is i32 (treated as unsigned), compare with 0, convert result to i8
            let zero = ctx.builder.ins().iconst(types::I32, 0);
            let cmp = ctx.builder.ins().icmp(IntCC::NotEqual, val, zero);
            let one = ctx.builder.ins().iconst(types::I8, 1);
            let zero_i8 = ctx.builder.ins().iconst(types::I8, 0);
            Ok(ctx.builder.ins().select(cmp, one, zero_i8))
        }
        // int ↔ uint conversions (bit pattern preserved, no-op)
        (GlslType::Int, GlslType::UInt) | (GlslType::UInt, GlslType::Int) => {
            // Bit pattern preservation: no conversion needed, just type change
            // Both use I32 in Cranelift, operations differ based on type
            Ok(val)
        }
        // float ↔ uint conversions
        (GlslType::Float, GlslType::UInt) => {
            // float → uint: truncates fractional part toward zero (undefined for negative)
            // val is f32, convert to i32 using fcvt_to_uint
            Ok(ctx.builder.ins().fcvt_to_uint(types::I32, val))
        }
        (GlslType::UInt, GlslType::Float) => {
            // uint → float: convert unsigned to float
            // val is i32 (treated as unsigned), convert to f32 using fcvt_from_uint
            Ok(ctx.builder.ins().fcvt_from_uint(types::F32, val))
        }
        _ => {
            let error_msg = format!("cannot implicitly convert {from_ty:?} to {to_ty:?}");
            let mut error = GlslError::new(ErrorCode::E0103, error_msg);
            if let Some(s) = span {
                error = error.with_location(source_span_to_location(&s));
            }
            Err(error)
        }
    }
}
