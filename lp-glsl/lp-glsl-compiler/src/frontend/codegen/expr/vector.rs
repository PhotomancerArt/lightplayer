use crate::error::{ErrorCode, GlslError, source_span_to_location};
use crate::frontend::codegen::context::CodegenContext;
use crate::semantic::types::Type as GlslType;
use cranelift_codegen::ir::{InstBuilder, Value};

use super::binary;
use super::coercion;

use alloc::{format, vec, vec::Vec};

pub fn emit_vector_binary<M: cranelift_module::Module>(
    ctx: &mut CodegenContext<'_, M>,
    op: &glsl::syntax::BinaryOp,
    lhs_vals: Vec<Value>,
    lhs_ty: &GlslType,
    rhs_vals: Vec<Value>,
    rhs_ty: &GlslType,
    span: Option<glsl::syntax::SourceSpan>,
) -> Result<(Vec<Value>, GlslType), GlslError> {
    use glsl::syntax::BinaryOp::*;

    // Validate operation is allowed on vectors
    match op {
        Add | Sub | Mult | Div | Mod => {} // arithmetic operations
        Equal | NonEqual => {} // comparison operations (aggregate comparison, returns bool)
        _ => {
            return Err(GlslError::new(
                ErrorCode::E0400,
                format!("operator {op:?} not supported on vectors yet"),
            ));
        }
    }

    // Determine result type and operation mode
    enum VectorOpMode {
        ComponentWise, // vec + vec
        VectorScalar,  // vec + scalar
        ScalarVector,  // scalar + vec
    }

    let (result_ty, mode) = if lhs_ty.is_vector() && rhs_ty.is_vector() {
        // vec + vec: component-wise, types must match
        if lhs_ty != rhs_ty {
            let mut error = GlslError::new(
                ErrorCode::E0106,
                format!("vector operation requires matching types, got {lhs_ty:?} and {rhs_ty:?}"),
            );
            if let Some(s) = span {
                error = error.with_location(source_span_to_location(&s));
            }
            return Err(error);
        }
        (lhs_ty.clone(), VectorOpMode::ComponentWise)
    } else if lhs_ty.is_vector() && rhs_ty.is_scalar() {
        // vec + scalar: scalar applied to each component
        let vec_base = lhs_ty.vector_base_type().unwrap();
        // Check if scalar can be used with vector base type
        if !rhs_ty.is_numeric() || !vec_base.is_numeric() {
            return Err(GlslError::new(
                ErrorCode::E0106,
                format!("cannot use {rhs_ty:?} with {lhs_ty:?}"),
            ));
        }
        (lhs_ty.clone(), VectorOpMode::VectorScalar)
    } else if lhs_ty.is_scalar() && rhs_ty.is_vector() {
        // scalar + vec: scalar applied to each component
        let vec_base = rhs_ty.vector_base_type().unwrap();
        if !lhs_ty.is_numeric() || !vec_base.is_numeric() {
            return Err(GlslError::new(
                ErrorCode::E0400,
                format!("Cannot use {lhs_ty:?} with {rhs_ty:?}"),
            ));
        }
        (rhs_ty.clone(), VectorOpMode::ScalarVector)
    } else {
        unreachable!("translate_vector_binary called with non-vector types");
    };

    let base_ty = result_ty.vector_base_type().unwrap();
    let component_count = result_ty.component_count().unwrap();

    // Handle comparison operators specially - they return scalar bool (aggregate comparison)
    if matches!(
        op,
        glsl::syntax::BinaryOp::Equal | glsl::syntax::BinaryOp::NonEqual
    ) {
        if !matches!(mode, VectorOpMode::ComponentWise) {
            return Err(GlslError::new(
                ErrorCode::E0400,
                "comparison operators require matching vector types",
            ));
        }
        // Aggregate comparison: compare all components and return bool (true if all equal)
        let zero = ctx
            .builder
            .ins()
            .iconst(cranelift_codegen::ir::types::I8, 0);
        let one = ctx
            .builder
            .ins()
            .iconst(cranelift_codegen::ir::types::I8, 1);

        // Start with true (all components equal so far)
        let mut all_equal_cmp: Option<cranelift_codegen::ir::Value> = None;

        for i in 0..component_count {
            let lhs_comp = lhs_vals[i];
            let rhs_comp = rhs_vals[i];
            // Compare components (returns I1)
            let cmp = if base_ty == GlslType::Bool
                || base_ty == GlslType::Int
                || base_ty == GlslType::UInt
            {
                ctx.builder.ins().icmp(
                    cranelift_codegen::ir::condcodes::IntCC::Equal,
                    lhs_comp,
                    rhs_comp,
                )
            } else {
                ctx.builder.ins().fcmp(
                    cranelift_codegen::ir::condcodes::FloatCC::Equal,
                    lhs_comp,
                    rhs_comp,
                )
            };
            // AND with previous result (band works on I1)
            if let Some(prev) = all_equal_cmp {
                all_equal_cmp = Some(ctx.builder.ins().band(prev, cmp));
            } else {
                all_equal_cmp = Some(cmp);
            }
        }

        let all_equal = all_equal_cmp.unwrap();

        // For ==, return all_equal; for !=, return NOT(all_equal)
        if matches!(op, glsl::syntax::BinaryOp::Equal) {
            // ==: return one if all equal, zero otherwise
            let result = ctx.builder.ins().select(all_equal, one, zero);
            return Ok((vec![result], GlslType::Bool));
        } else {
            // !=: return zero if all equal, one otherwise (swapped arguments)
            let result = ctx.builder.ins().select(all_equal, zero, one);
            return Ok((vec![result], GlslType::Bool));
        }
    }

    let mut result_vals = Vec::new();

    // Use provided span or create a default one if not available
    let span = span.unwrap_or_else(|| glsl::syntax::SourceSpan::unknown());

    match mode {
        VectorOpMode::ComponentWise => {
            // vec3(a,b,c) + vec3(d,e,f) = vec3(a+d, b+e, c+f)
            for i in 0..component_count {
                let lhs_comp = lhs_vals[i];
                let rhs_comp = rhs_vals[i];
                let result_comp = binary::emit_scalar_binary_op_internal(
                    ctx,
                    op,
                    lhs_comp,
                    rhs_comp,
                    &base_ty,
                    span.clone(),
                )?;
                result_vals.push(result_comp);
            }
        }
        VectorOpMode::VectorScalar => {
            // vec3(a,b,c) * s = vec3(a*s, b*s, c*s)
            let scalar = rhs_vals[0];
            // Coerce scalar to vector base type if needed
            let scalar = coercion::coerce_to_type(ctx, scalar, rhs_ty, &base_ty)?;
            for &comp in &lhs_vals {
                let result_comp = binary::emit_scalar_binary_op_internal(
                    ctx,
                    op,
                    comp,
                    scalar,
                    &base_ty,
                    span.clone(),
                )?;
                result_vals.push(result_comp);
            }
        }
        VectorOpMode::ScalarVector => {
            // s * vec3(a,b,c) = vec3(s*a, s*b, s*c)
            let scalar = lhs_vals[0];
            // Coerce scalar to vector base type if needed
            let scalar = coercion::coerce_to_type(ctx, scalar, lhs_ty, &base_ty)?;
            for &comp in &rhs_vals {
                let result_comp = binary::emit_scalar_binary_op_internal(
                    ctx,
                    op,
                    scalar,
                    comp,
                    &base_ty,
                    span.clone(),
                )?;
                result_vals.push(result_comp);
            }
        }
    }

    Ok((result_vals, result_ty))
}
