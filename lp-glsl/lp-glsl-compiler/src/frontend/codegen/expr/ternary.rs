use crate::error::{ErrorCode, GlslError, extract_span_from_expr, source_span_to_location};
use crate::frontend::codegen::context::CodegenContext;
use crate::frontend::codegen::rvalue::RValue;
use crate::frontend::semantic::type_check::conversion::can_implicitly_convert;
use crate::semantic::types::Type as GlslType;
use cranelift_codegen::ir::{InstBuilder, Value, condcodes::IntCC, types};
use glsl::syntax::Expr;

use super::coercion;

use alloc::{format, vec, vec::Vec};

/// Emit code to compute a ternary expression as an RValue
pub fn emit_ternary_rvalue<M: cranelift_module::Module>(
    ctx: &mut CodegenContext<'_, M>,
    expr: &Expr,
) -> Result<RValue, GlslError> {
    // Ensure we're in a block before evaluating
    ctx.ensure_block()?;

    let Expr::Ternary(cond, true_expr, false_expr, span) = expr else {
        unreachable!("emit_ternary_rvalue called on non-ternary expr");
    };

    // Emit condition and validate it's scalar bool
    let cond_rvalue = ctx.emit_rvalue(cond)?;
    let cond_ty = cond_rvalue.ty().clone();
    let cond_vals = cond_rvalue.into_values();

    // Validate condition is scalar bool
    if cond_ty != GlslType::Bool {
        let cond_span = extract_span_from_expr(cond);
        return Err(GlslError::new(
            ErrorCode::E0107,
            "ternary condition must be scalar bool type",
        )
        .with_location(source_span_to_location(&cond_span))
        .with_note(format!("condition has type `{cond_ty:?}`, expected `Bool`")));
    }

    // Condition must be scalar, so take the first (and only) value
    let cond_span = extract_span_from_expr(cond);
    let cond_val = cond_vals.into_iter().next().ok_or_else(|| {
        let error = GlslError::new(ErrorCode::E0400, "condition expression produced no value")
            .with_location(source_span_to_location(&cond_span));
        ctx.add_span_to_error(error, &cond_span)
    })?;

    // Emit both branches
    let true_rvalue = ctx.emit_rvalue(true_expr)?;
    let false_rvalue = ctx.emit_rvalue(false_expr)?;
    let true_ty = true_rvalue.ty().clone();
    let false_ty = false_rvalue.ty().clone();
    let mut true_vals = true_rvalue.into_values();
    let mut false_vals = false_rvalue.into_values();

    // Determine result type using same logic as type inference
    let result_ty = if true_ty == false_ty {
        // Exact match
        true_ty.clone()
    } else if can_implicitly_convert(&true_ty, &false_ty) {
        // true_ty can convert to false_ty, use false_ty as result
        false_ty.clone()
    } else if can_implicitly_convert(&false_ty, &true_ty) {
        // false_ty can convert to true_ty, use true_ty as result
        true_ty.clone()
    } else {
        // No conversion possible
        return Err(GlslError::new(
            ErrorCode::E0106,
            "ternary operator branches have incompatible types",
        )
        .with_location(source_span_to_location(span))
        .with_note(format!(
            "true branch has type `{true_ty:?}`, false branch has type `{false_ty:?}`"
        ))
        .with_note("branches must have matching types or allow implicit conversion"));
    };

    // Coerce branches to result type if needed
    if true_ty != result_ty {
        // Coerce true branch component-wise
        let true_base = if true_ty.is_vector() {
            true_ty.vector_base_type().unwrap()
        } else if true_ty.is_matrix() {
            GlslType::Float // Matrices are always float
        } else {
            true_ty.clone()
        };
        let result_base = if result_ty.is_vector() {
            result_ty.vector_base_type().unwrap()
        } else if result_ty.is_matrix() {
            GlslType::Float // Matrices are always float
        } else {
            result_ty.clone()
        };
        let true_count = true_vals.len();
        let mut coerced_true_vals = Vec::new();
        for i in 0..true_count {
            let coerced = coercion::coerce_to_type(ctx, true_vals[i], &true_base, &result_base)?;
            coerced_true_vals.push(coerced);
        }
        true_vals = coerced_true_vals;
    }

    if false_ty != result_ty {
        // Coerce false branch component-wise
        let false_base = if false_ty.is_vector() {
            false_ty.vector_base_type().unwrap()
        } else if false_ty.is_matrix() {
            GlslType::Float // Matrices are always float
        } else {
            false_ty.clone()
        };
        let result_base = if result_ty.is_vector() {
            result_ty.vector_base_type().unwrap()
        } else if result_ty.is_matrix() {
            GlslType::Float // Matrices are always float
        } else {
            result_ty.clone()
        };
        let false_count = false_vals.len();
        let mut coerced_false_vals = Vec::new();
        for i in 0..false_count {
            let coerced = coercion::coerce_to_type(ctx, false_vals[i], &false_base, &result_base)?;
            coerced_false_vals.push(coerced);
        }
        false_vals = coerced_false_vals;
    }

    // Now both branches have the same type (result_ty)
    // Handle different cases: scalar, vector, matrix

    if result_ty.is_matrix() {
        // Matrix: select component-wise
        let (vals, ty) = emit_matrix_ternary(
            ctx,
            cond_val,
            true_vals,
            false_vals,
            &result_ty,
            span.clone(),
        )?;
        Ok(RValue::from_aggregate(vals, ty))
    } else if result_ty.is_vector() {
        // Vector: select component-wise
        let (vals, ty) = emit_vector_ternary(
            ctx,
            cond_val,
            true_vals,
            false_vals,
            &result_ty,
            span.clone(),
        )?;
        Ok(RValue::from_aggregate(vals, ty))
    } else {
        // Scalar: use select directly
        let (vals, ty) = emit_scalar_ternary(
            ctx,
            cond_val,
            true_vals[0],
            false_vals[0],
            &result_ty,
            span.clone(),
        )?;
        Ok(RValue::from_aggregate(vals, ty))
    }
}

fn emit_scalar_ternary<M: cranelift_module::Module>(
    ctx: &mut CodegenContext<'_, M>,
    cond_val: Value,
    true_val: Value,
    false_val: Value,
    result_ty: &GlslType,
    _span: glsl::syntax::SourceSpan,
) -> Result<(Vec<Value>, GlslType), GlslError> {
    // For scalar, use Cranelift's select instruction directly
    // cond_val is i8 (bool), need to convert to I1 for select
    let zero = ctx.builder.ins().iconst(types::I8, 0);
    let cond_i1 = ctx.builder.ins().icmp(IntCC::NotEqual, cond_val, zero);
    let result_val = ctx.builder.ins().select(cond_i1, true_val, false_val);
    Ok((vec![result_val], result_ty.clone()))
}

fn emit_vector_ternary<M: cranelift_module::Module>(
    ctx: &mut CodegenContext<'_, M>,
    cond_val: Value,
    true_vals: Vec<Value>,
    false_vals: Vec<Value>,
    result_ty: &GlslType,
    _span: glsl::syntax::SourceSpan,
) -> Result<(Vec<Value>, GlslType), GlslError> {
    // Vector: select each component independently
    // cond_val is scalar bool (i8), convert to I1 for select
    let zero = ctx.builder.ins().iconst(types::I8, 0);
    let cond_i1 = ctx.builder.ins().icmp(IntCC::NotEqual, cond_val, zero);

    let component_count = result_ty.component_count().unwrap();
    let mut result_vals = Vec::new();

    for i in 0..component_count {
        let result_comp = ctx
            .builder
            .ins()
            .select(cond_i1, true_vals[i], false_vals[i]);
        result_vals.push(result_comp);
    }

    Ok((result_vals, result_ty.clone()))
}

fn emit_matrix_ternary<M: cranelift_module::Module>(
    ctx: &mut CodegenContext<'_, M>,
    cond_val: Value,
    true_vals: Vec<Value>,
    false_vals: Vec<Value>,
    result_ty: &GlslType,
    _span: glsl::syntax::SourceSpan,
) -> Result<(Vec<Value>, GlslType), GlslError> {
    // Matrix: select each component independently
    // cond_val is scalar bool (i8), convert to I1 for select
    let zero = ctx.builder.ins().iconst(types::I8, 0);
    let cond_i1 = ctx.builder.ins().icmp(IntCC::NotEqual, cond_val, zero);

    let component_count = true_vals.len();
    let mut result_vals = Vec::new();

    for i in 0..component_count {
        let result_comp = ctx
            .builder
            .ins()
            .select(cond_i1, true_vals[i], false_vals[i]);
        result_vals.push(result_comp);
    }

    Ok((result_vals, result_ty.clone()))
}
