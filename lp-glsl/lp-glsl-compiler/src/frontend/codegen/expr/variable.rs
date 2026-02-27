use crate::error::{GlslError, extract_span_from_identifier, source_span_to_location};
use crate::frontend::codegen::context::CodegenContext;
use crate::frontend::codegen::lvalue::emit_lvalue_as_rvalue;
use crate::frontend::codegen::rvalue::RValue;
use crate::semantic::const_eval::ConstValue;
use crate::semantic::types::Type as GlslType;
use cranelift_codegen::ir::{InstBuilder, Value, types};
use glsl::syntax::Expr;

use alloc::vec::Vec;

pub fn emit_variable<M: cranelift_module::Module>(
    ctx: &mut CodegenContext<'_, M>,
    expr: &Expr,
) -> Result<(Vec<Value>, GlslType), GlslError> {
    let Expr::Variable(ident, _span) = expr else {
        unreachable!("translate_variable called on non-variable");
    };

    let span = extract_span_from_identifier(ident);

    // Get variable type first to check if it's an array
    let ty = ctx
        .lookup_variable_type(&ident.name)
        .ok_or_else(|| {
            let error = GlslError::undefined_variable(&ident.name)
                .with_location(source_span_to_location(&span));
            ctx.add_span_to_error(error, &span)
        })?
        .clone();

    // For arrays, we can't evaluate them as RValues directly (they need indexing)
    // Return empty vec but correct type so type checking works
    if ty.is_array() {
        return Ok((Vec::new(), ty));
    }

    let vars = ctx
        .lookup_variables(&ident.name)
        .ok_or_else(|| {
            let error = GlslError::undefined_variable(&ident.name)
                .with_location(source_span_to_location(&span));
            ctx.add_span_to_error(error, &span)
        })?
        .to_vec(); // Clone to avoid borrow issues

    // Ensure we're in the correct block before reading variables
    // This is important when reading variables in merge blocks after control flow
    ctx.ensure_block()?;
    ctx.builder.ensure_inserted_block();

    // Read all component values fresh in the current block context
    // This ensures we get the correct SSA values for the current block
    let vals: Vec<Value> = vars
        .iter()
        .map(|&v| {
            // Force a fresh read of the variable in the current block
            ctx.builder.use_var(v)
        })
        .collect();

    Ok((vals, ty))
}

/// Emit variable expression as RValue
///
/// If the variable is a global const, emit its value directly.
/// Otherwise resolve as LValue and load.
pub fn emit_variable_rvalue<M: cranelift_module::Module>(
    ctx: &mut CodegenContext<'_, M>,
    expr: &Expr,
) -> Result<RValue, GlslError> {
    let Expr::Variable(ident, _) = expr else {
        unreachable!("emit_variable_rvalue called on non-variable");
    };
    if let Some(constants) = ctx.global_constants {
        if let Some(val) = constants.get(&ident.name) {
            return emit_const_value_rvalue(ctx, val);
        }
    }
    if let Some(val) = ctx.local_const_env.get(&ident.name).cloned() {
        return emit_const_value_rvalue(ctx, &val);
    }
    emit_lvalue_as_rvalue(ctx, expr)
}

/// Emit a ConstValue as Cranelift IR and wrap in RValue.
fn emit_const_value_rvalue<M: cranelift_module::Module>(
    ctx: &mut CodegenContext<'_, M>,
    val: &ConstValue,
) -> Result<RValue, GlslError> {
    let vals = match val {
        ConstValue::Int(n) => vec![ctx.builder.ins().iconst(types::I32, *n as i64)],
        ConstValue::UInt(n) => vec![ctx.builder.ins().iconst(types::I32, *n as i64)],
        ConstValue::Float(f) => vec![ctx.builder.ins().f32const(*f)],
        ConstValue::Bool(b) => vec![ctx.builder.ins().iconst(types::I8, if *b { 1 } else { 0 })],
        ConstValue::Vec2(v) => vec![
            ctx.builder.ins().f32const(v[0]),
            ctx.builder.ins().f32const(v[1]),
        ],
        ConstValue::Vec3(v) => vec![
            ctx.builder.ins().f32const(v[0]),
            ctx.builder.ins().f32const(v[1]),
            ctx.builder.ins().f32const(v[2]),
        ],
        ConstValue::Vec4(v) => vec![
            ctx.builder.ins().f32const(v[0]),
            ctx.builder.ins().f32const(v[1]),
            ctx.builder.ins().f32const(v[2]),
            ctx.builder.ins().f32const(v[3]),
        ],
        ConstValue::IVec2(v) => vec![
            ctx.builder.ins().iconst(types::I32, v[0] as i64),
            ctx.builder.ins().iconst(types::I32, v[1] as i64),
        ],
        ConstValue::IVec3(v) => vec![
            ctx.builder.ins().iconst(types::I32, v[0] as i64),
            ctx.builder.ins().iconst(types::I32, v[1] as i64),
            ctx.builder.ins().iconst(types::I32, v[2] as i64),
        ],
        ConstValue::IVec4(v) => vec![
            ctx.builder.ins().iconst(types::I32, v[0] as i64),
            ctx.builder.ins().iconst(types::I32, v[1] as i64),
            ctx.builder.ins().iconst(types::I32, v[2] as i64),
            ctx.builder.ins().iconst(types::I32, v[3] as i64),
        ],
        ConstValue::UVec2(v) => vec![
            ctx.builder.ins().iconst(types::I32, v[0] as i64),
            ctx.builder.ins().iconst(types::I32, v[1] as i64),
        ],
        ConstValue::UVec3(v) => vec![
            ctx.builder.ins().iconst(types::I32, v[0] as i64),
            ctx.builder.ins().iconst(types::I32, v[1] as i64),
            ctx.builder.ins().iconst(types::I32, v[2] as i64),
        ],
        ConstValue::UVec4(v) => vec![
            ctx.builder.ins().iconst(types::I32, v[0] as i64),
            ctx.builder.ins().iconst(types::I32, v[1] as i64),
            ctx.builder.ins().iconst(types::I32, v[2] as i64),
            ctx.builder.ins().iconst(types::I32, v[3] as i64),
        ],
        ConstValue::BVec2(v) => vec![
            ctx.builder
                .ins()
                .iconst(types::I8, if v[0] { 1 } else { 0 }),
            ctx.builder
                .ins()
                .iconst(types::I8, if v[1] { 1 } else { 0 }),
        ],
        ConstValue::BVec3(v) => vec![
            ctx.builder
                .ins()
                .iconst(types::I8, if v[0] { 1 } else { 0 }),
            ctx.builder
                .ins()
                .iconst(types::I8, if v[1] { 1 } else { 0 }),
            ctx.builder
                .ins()
                .iconst(types::I8, if v[2] { 1 } else { 0 }),
        ],
        ConstValue::BVec4(v) => vec![
            ctx.builder
                .ins()
                .iconst(types::I8, if v[0] { 1 } else { 0 }),
            ctx.builder
                .ins()
                .iconst(types::I8, if v[1] { 1 } else { 0 }),
            ctx.builder
                .ins()
                .iconst(types::I8, if v[2] { 1 } else { 0 }),
            ctx.builder
                .ins()
                .iconst(types::I8, if v[3] { 1 } else { 0 }),
        ],
        ConstValue::Mat2(m) => vec![
            ctx.builder.ins().f32const(m[0][0]),
            ctx.builder.ins().f32const(m[0][1]),
            ctx.builder.ins().f32const(m[1][0]),
            ctx.builder.ins().f32const(m[1][1]),
        ],
    };
    Ok(RValue::from_aggregate(vals, val.glsl_type()))
}
