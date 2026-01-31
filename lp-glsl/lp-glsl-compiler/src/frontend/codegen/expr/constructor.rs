use crate::error::{ErrorCode, GlslError, source_span_to_location};
use crate::frontend::codegen::context::CodegenContext;
use crate::semantic::type_check::{check_matrix_constructor, check_vector_constructor_with_span};
use crate::semantic::types::Type as GlslType;
use cranelift_codegen::ir::InstBuilder;
use glsl::syntax::Expr;

use super::coercion;

use alloc::{format, vec, vec::Vec};

pub fn emit_vector_constructor<M: cranelift_module::Module>(
    ctx: &mut CodegenContext<'_, M>,
    type_name: &str,
    args: &[Expr],
    span: glsl::syntax::SourceSpan,
) -> Result<(Vec<cranelift_codegen::ir::Value>, GlslType), GlslError> {
    // Ensure we're in a block before evaluating
    ctx.ensure_block()?;

    // Translate all arguments
    let mut arg_vals: Vec<Vec<cranelift_codegen::ir::Value>> = Vec::new();
    let mut arg_types: Vec<GlslType> = Vec::new();

    for arg in args {
        let (vals, ty) = ctx.emit_expr_typed(arg)?;
        arg_vals.push(vals);
        arg_types.push(ty);
    }

    // Type check constructor
    let result_type =
        match check_vector_constructor_with_span(type_name, &arg_types, Some(span.clone())) {
            Ok(ty) => ty,
            Err(mut error) => {
                // Ensure error has location and span_text
                if error.location.is_none() {
                    error = error.with_location(source_span_to_location(&span));
                }
                return Err(ctx.add_span_to_error(error, &span));
            }
        };
    let base_type = result_type.vector_base_type().unwrap();
    let component_count = result_type.component_count().unwrap();
    crate::debug!(
        "vector constructor: type_name={}, result_type={:?}, base_type={:?}, component_count={}",
        type_name,
        result_type,
        base_type,
        component_count
    );

    // Generate component values
    let mut components = Vec::new();

    // Case 1: Single scalar broadcast
    if arg_types.len() == 1 && arg_types[0].is_scalar() {
        crate::debug!("  Case 1: Single scalar broadcast");
        let scalar = arg_vals[0][0];
        let coerced = coercion::coerce_to_type(ctx, scalar, &arg_types[0], &base_type)?;
        for _ in 0..component_count {
            components.push(coerced);
        }
    }
    // Case 2: Single vector conversion (including shortening)
    else if arg_types.len() == 1 && arg_types[0].is_vector() {
        crate::debug!("  Case 2: Single vector conversion");
        let src_base = arg_types[0].vector_base_type().unwrap();
        // For shortening, only take the first component_count components
        for i in 0..component_count {
            let val = arg_vals[0][i];
            components.push(coercion::coerce_to_type(ctx, val, &src_base, &base_type)?);
        }
    }
    // Case 3: Concatenation
    else {
        crate::debug!("  Case 3: Concatenation, {} args", arg_types.len());
        for (vals, ty) in arg_vals.iter().zip(&arg_types) {
            let arg_base = if ty.is_vector() {
                ty.vector_base_type().unwrap()
            } else {
                ty.clone()
            };

            for &val in vals {
                components.push(coercion::coerce_to_type(ctx, val, &arg_base, &base_type)?);
            }
        }
    }

    Ok((components, result_type))
}

/// Translate matrix constructor
/// Implements GLSL spec: variables.adoc:72-97
pub fn emit_matrix_constructor<M: cranelift_module::Module>(
    ctx: &mut CodegenContext<'_, M>,
    type_name: &str,
    args: &[Expr],
) -> Result<(Vec<cranelift_codegen::ir::Value>, GlslType), GlslError> {
    // Ensure we're in a block before evaluating
    ctx.ensure_block()?;

    // Translate all arguments
    let mut arg_vals: Vec<Vec<cranelift_codegen::ir::Value>> = Vec::new();
    let mut arg_types: Vec<GlslType> = Vec::new();

    for arg in args {
        let (vals, ty) = ctx.emit_expr_typed(arg)?;
        arg_vals.push(vals);
        arg_types.push(ty);
    }

    // Type check constructor
    let result_type = check_matrix_constructor(type_name, &arg_types)?;
    let (rows, cols) = result_type.matrix_dims().unwrap();
    let element_count = rows * cols;

    // Allocate temporary variables for matrix elements
    let mut matrix_vars = Vec::new();
    for _ in 0..element_count {
        let var = ctx.builder.declare_var(cranelift_codegen::ir::types::F32);
        matrix_vars.push(var);
    }

    // Case 1: Single scalar - identity matrix (diagonal = scalar, rest = 0.0)
    if arg_types.len() == 1 && arg_types[0].is_scalar() {
        let scalar = arg_vals[0][0];
        let scalar_float = coercion::coerce_to_type(ctx, scalar, &arg_types[0], &GlslType::Float)?;
        let zero = ctx.builder.ins().f32const(0.0);

        for row in 0..rows {
            for col in 0..cols {
                let value = if row == col { scalar_float } else { zero };
                ctx.builder.def_var(matrix_vars[col * rows + row], value);
            }
        }
    }
    // Case 2: Column vectors - one vector per column
    else if arg_types.len() == cols {
        for (col, (vals, ty)) in arg_vals.iter().zip(&arg_types).enumerate() {
            let vec_base = ty.vector_base_type().unwrap();
            for (row, &val) in vals.iter().enumerate() {
                let float_val = coercion::coerce_to_type(ctx, val, &vec_base, &GlslType::Float)?;
                ctx.builder
                    .def_var(matrix_vars[col * rows + row], float_val);
            }
        }
    }
    // Case 3: Single matrix - conversion between matrix sizes
    else if arg_types.len() == 1 && arg_types[0].is_matrix() {
        let src_matrix_vals = &arg_vals[0];
        let src_ty = &arg_types[0];
        let (src_rows, src_cols) = src_ty.matrix_dims().unwrap();

        // Copy elements from source matrix, padding with identity/truncating as needed
        for col in 0..cols {
            for row in 0..rows {
                let value = if col < src_cols && row < src_rows {
                    // Copy from source matrix
                    let src_idx = col * src_rows + row;
                    src_matrix_vals[src_idx]
                } else if col == row {
                    // Identity padding (diagonal = 1.0)
                    ctx.builder.ins().f32const(1.0)
                } else {
                    // Off-diagonal padding = 0.0
                    ctx.builder.ins().f32const(0.0)
                };
                let var_idx = col * rows + row;
                ctx.builder.def_var(matrix_vars[var_idx], value);
            }
        }
    }
    // Case 4: Mixed scalars and/or vectors - column-major order
    else {
        let mut element_index = 0;
        for (arg_val, arg_ty) in arg_vals.iter().zip(&arg_types) {
            if arg_ty.is_vector() {
                // Vector contributes all its elements as a column (or part of a column)
                let vec_base = arg_ty.vector_base_type().unwrap();
                for &val in arg_val {
                    if element_index >= element_count {
                        break;
                    }
                    let col = element_index / rows;
                    let row = element_index % rows;
                    let float_val =
                        coercion::coerce_to_type(ctx, val, &vec_base, &GlslType::Float)?;
                    let var_idx = col * rows + row;
                    ctx.builder.def_var(matrix_vars[var_idx], float_val);
                    element_index += 1;
                }
            } else if arg_ty.is_scalar() {
                // Scalar contributes one element
                if element_index >= element_count {
                    break;
                }
                let col = element_index / rows;
                let row = element_index % rows;
                let float_val =
                    coercion::coerce_to_type(ctx, arg_val[0], arg_ty, &GlslType::Float)?;
                let var_idx = col * rows + row;
                ctx.builder.def_var(matrix_vars[var_idx], float_val);
                element_index += 1;
            }
        }
    }

    // Return all matrix element values
    let mut result_vals = Vec::new();
    for &var in &matrix_vars {
        result_vals.push(ctx.builder.use_var(var));
    }

    Ok((result_vals, result_type))
}

/// Translate scalar type constructor (bool(int), int(bool), etc.)
pub fn emit_scalar_constructor<M: cranelift_module::Module>(
    ctx: &mut CodegenContext<'_, M>,
    type_name: &str,
    args: &[Expr],
    span: glsl::syntax::SourceSpan,
) -> Result<(Vec<cranelift_codegen::ir::Value>, GlslType), GlslError> {
    // Ensure we're in a block before evaluating
    ctx.ensure_block()?;

    // Scalar constructors take exactly one argument
    if args.len() != 1 {
        return Err(GlslError::new(
            ErrorCode::E0115,
            format!("`{type_name}` constructor requires exactly one argument"),
        )
        .with_location(source_span_to_location(&span)));
    }

    // Translate argument
    let arg_rvalue = ctx.emit_rvalue(&args[0])?;
    let arg_ty = arg_rvalue.ty().clone();
    let arg_vals = arg_rvalue.into_values();

    // Extract first component (for vectors) or use scalar value
    let arg_val = if arg_vals.len() > 1 {
        // Vector: extract first component
        arg_vals[0]
    } else if arg_vals.len() == 1 {
        // Scalar
        arg_vals[0]
    } else {
        return Err(GlslError::new(
            ErrorCode::E0115,
            format!("`{type_name}` constructor requires at least one component"),
        )
        .with_location(source_span_to_location(&span)));
    };

    // For vectors, get the base type for coercion
    let arg_base_ty = if arg_ty.is_vector() {
        arg_ty.vector_base_type().unwrap()
    } else {
        arg_ty.clone()
    };

    // Determine result type
    let result_ty = match type_name {
        "bool" => GlslType::Bool,
        "int" => GlslType::Int,
        "uint" => GlslType::UInt,
        "float" => GlslType::Float,
        _ => {
            return Err(GlslError::new(
                ErrorCode::E0112,
                format!("`{type_name}` is not a scalar type"),
            )
            .with_location(source_span_to_location(&span)));
        }
    };

    // Convert argument to result type using coercion (use base type for vectors)
    let result_val = coercion::coerce_to_type_with_location(
        ctx,
        arg_val,
        &arg_base_ty,
        &result_ty,
        Some(span.clone()),
    )?;

    Ok((vec![result_val], result_ty))
}
