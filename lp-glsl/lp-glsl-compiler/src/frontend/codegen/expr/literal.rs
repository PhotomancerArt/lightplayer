use crate::error::GlslError;
use crate::frontend::codegen::context::CodegenContext;
use crate::frontend::codegen::rvalue::RValue;
use crate::semantic::types::Type as GlslType;
use cranelift_codegen::ir::{InstBuilder, types};
use glsl::syntax::Expr;

/// Emit code to compute a literal as an RValue
pub fn emit_literal_rvalue<M: cranelift_module::Module>(
    ctx: &mut CodegenContext<'_, M>,
    expr: &Expr,
) -> Result<RValue, GlslError> {
    match expr {
        Expr::IntConst(n, _) => {
            let val = ctx.builder.ins().iconst(types::I32, *n as i64);
            Ok(RValue::from_scalar(val, GlslType::Int))
        }
        Expr::UIntConst(n, _) => {
            // UIntConst stores value as u32, preserve bit pattern by casting to i64
            let val = ctx.builder.ins().iconst(types::I32, *n as i64);
            Ok(RValue::from_scalar(val, GlslType::UInt))
        }
        Expr::FloatConst(f, _) => {
            let val = ctx.builder.ins().f32const(*f);
            Ok(RValue::from_scalar(val, GlslType::Float))
        }
        Expr::BoolConst(b, _) => {
            let val = ctx.builder.ins().iconst(types::I8, if *b { 1 } else { 0 });
            Ok(RValue::from_scalar(val, GlslType::Bool))
        }
        _ => unreachable!("emit_literal_rvalue called on non-literal"),
    }
}

/// TODO Legacy function for backwards compatibility
use alloc::vec::Vec;

pub fn emit_literal<M: cranelift_module::Module>(
    ctx: &mut CodegenContext<'_, M>,
    expr: &Expr,
) -> Result<(Vec<cranelift_codegen::ir::Value>, GlslType), GlslError> {
    let rvalue = emit_literal_rvalue(ctx, expr)?;
    let ty = rvalue.ty().clone();
    Ok((rvalue.into_values(), ty))
}
