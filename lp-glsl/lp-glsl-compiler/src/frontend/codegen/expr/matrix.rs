use crate::error::{ErrorCode, GlslError, source_span_to_location};
use crate::frontend::codegen::context::CodegenContext;
use crate::semantic::type_check::operators::infer_binary_result_type;
use crate::semantic::types::Type as GlslType;
use cranelift_codegen::ir::{InstBuilder, Value};

use super::coercion;

use alloc::{format, vec, vec::Vec};

pub fn emit_matrix_binary<M: cranelift_module::Module>(
    ctx: &mut CodegenContext<'_, M>,
    op: &glsl::syntax::BinaryOp,
    lhs_vals: Vec<Value>,
    lhs_ty: &GlslType,
    rhs_vals: Vec<Value>,
    rhs_ty: &GlslType,
    span: glsl::syntax::SourceSpan,
) -> Result<(Vec<Value>, GlslType), GlslError> {
    use glsl::syntax::BinaryOp::*;

    // Infer result type (validates operation is allowed)
    let result_ty = infer_binary_result_type(op, lhs_ty, rhs_ty, span.clone())?;

    match op {
        // Matrix + Matrix: component-wise addition
        Add => {
            if lhs_ty != rhs_ty {
                return Err(GlslError::new(
                    ErrorCode::E0106,
                    "matrix addition requires matching types",
                )
                .with_location(source_span_to_location(&span)));
            }
            let mut result_vals = Vec::new();
            for (lhs_val, rhs_val) in lhs_vals.iter().zip(rhs_vals.iter()) {
                let result = ctx.builder.ins().fadd(*lhs_val, *rhs_val);
                result_vals.push(result);
            }
            Ok((result_vals, result_ty))
        }

        // Matrix - Matrix: component-wise subtraction
        Sub => {
            if lhs_ty != rhs_ty {
                return Err(GlslError::new(
                    ErrorCode::E0106,
                    "matrix subtraction requires matching types",
                )
                .with_location(source_span_to_location(&span)));
            }
            let mut result_vals = Vec::new();
            for (lhs_val, rhs_val) in lhs_vals.iter().zip(rhs_vals.iter()) {
                result_vals.push(ctx.builder.ins().fsub(*lhs_val, *rhs_val));
            }
            Ok((result_vals, result_ty))
        }

        // Matrix multiplication
        Mult => {
            // Matrix × Scalar: component-wise multiplication
            if lhs_ty.is_matrix() && rhs_ty.is_scalar() {
                let scalar = rhs_vals[0];
                let scalar_float = coercion::coerce_to_type(ctx, scalar, rhs_ty, &GlslType::Float)?;
                let mut result_vals = Vec::new();
                for &lhs_val in &lhs_vals {
                    result_vals.push(ctx.builder.ins().fmul(lhs_val, scalar_float));
                }
                return Ok((result_vals, result_ty));
            }

            // Scalar × Matrix: component-wise multiplication
            if lhs_ty.is_scalar() && rhs_ty.is_matrix() {
                let scalar = lhs_vals[0];
                let scalar_float = coercion::coerce_to_type(ctx, scalar, lhs_ty, &GlslType::Float)?;
                let mut result_vals = Vec::new();
                for &rhs_val in &rhs_vals {
                    result_vals.push(ctx.builder.ins().fmul(scalar_float, rhs_val));
                }
                return Ok((result_vals, result_ty));
            }

            // Matrix × Vector: linear algebra multiplication
            if lhs_ty.is_matrix() && rhs_ty.is_vector() {
                let (rows, cols) = lhs_ty.matrix_dims().unwrap();
                let vec_size = rhs_ty.component_count().unwrap();

                if cols != vec_size {
                    return Err(GlslError::new(ErrorCode::E0106,
                        format!("matrix × vector dimension mismatch: {rows}×{cols} matrix requires {cols}-component vector"))
                        .with_location(source_span_to_location(&span)));
                }

                // Result is a vector with rows components
                // For each row i: result[i] = dot(row i of matrix, vector)
                let mut result_vals = Vec::new();
                for row in 0..rows {
                    let mut sum = ctx.builder.ins().fmul(
                        lhs_vals[0 * rows + row], // First element of row
                        rhs_vals[0],
                    );
                    for col in 1..cols {
                        let product = ctx.builder.ins().fmul(
                            lhs_vals[col * rows + row], // Element at (row, col)
                            rhs_vals[col],
                        );
                        sum = ctx.builder.ins().fadd(sum, product);
                    }
                    result_vals.push(sum);
                }
                return Ok((result_vals, result_ty));
            }

            // Vector × Matrix: linear algebra multiplication
            if lhs_ty.is_vector() && rhs_ty.is_matrix() {
                let vec_size = lhs_ty.component_count().unwrap();
                let (rows, cols) = rhs_ty.matrix_dims().unwrap();

                if vec_size != rows {
                    return Err(GlslError::new(ErrorCode::E0106,
                        format!("vector × matrix dimension mismatch: {vec_size}-component vector requires {rows}×{cols} matrix"))
                        .with_location(source_span_to_location(&span)));
                }

                // Result is a vector with cols components
                // For each column j: result[j] = dot(vector, column j of matrix)
                let mut result_vals = Vec::new();
                for col in 0..cols {
                    let mut sum = ctx.builder.ins().fmul(
                        lhs_vals[0],
                        rhs_vals[col * rows + 0], // First element of column
                    );
                    for row in 1..rows {
                        let product = ctx.builder.ins().fmul(
                            lhs_vals[row],
                            rhs_vals[col * rows + row], // Element at (row, col)
                        );
                        sum = ctx.builder.ins().fadd(sum, product);
                    }
                    result_vals.push(sum);
                }
                return Ok((result_vals, result_ty));
            }

            // Matrix × Matrix: linear algebra multiplication
            if lhs_ty.is_matrix() && rhs_ty.is_matrix() {
                let (lhs_rows, lhs_cols) = lhs_ty.matrix_dims().unwrap();
                let (rhs_rows, rhs_cols) = rhs_ty.matrix_dims().unwrap();

                if lhs_cols != rhs_rows {
                    return Err(GlslError::new(
                        ErrorCode::E0106,
                        format!(
                            "matrix × matrix dimension mismatch: {lhs_rows}×{lhs_cols} × {rhs_rows}×{rhs_cols} requires {lhs_cols} == {rhs_rows}"
                        ),
                    )
                    .with_location(source_span_to_location(&span)));
                }

                // Result is lhs_rows × rhs_cols matrix
                // For each (i, j): result[i][j] = dot(row i of lhs, column j of rhs)
                //
                // Store result in column-major order: iterate by columns first, then rows
                // This ensures the output matches GLSL's column-major storage format.
                // Loop order: for j in 0..rhs_cols { for i in 0..lhs_rows { ... } }
                // produces: [col0_row0, col0_row1, ..., col1_row0, col1_row1, ...]
                let mut result_vals = Vec::new();
                for j in 0..rhs_cols {
                    for i in 0..lhs_rows {
                        // Dot product of row i of lhs with column j of rhs
                        // lhs element (i, k): lhs_vals[k * lhs_rows + i]
                        // rhs element (k, j): rhs_vals[j * rhs_rows + k]
                        let mut sum = ctx.builder.ins().fmul(
                            lhs_vals[0 * lhs_rows + i], // Element at (i, 0) of lhs
                            rhs_vals[j * rhs_rows + 0], // Element at (0, j) of rhs
                        );
                        for k in 1..lhs_cols {
                            let product = ctx.builder.ins().fmul(
                                lhs_vals[k * lhs_rows + i], // Element at (i, k) of lhs
                                rhs_vals[j * rhs_rows + k], // Element at (k, j) of rhs
                            );
                            sum = ctx.builder.ins().fadd(sum, product);
                        }
                        result_vals.push(sum);
                    }
                }
                return Ok((result_vals, result_ty));
            }

            Err(GlslError::new(
                ErrorCode::E0106,
                "matrix multiplication requires matrix and scalar/vector/matrix operands",
            )
            .with_location(source_span_to_location(&span)))
        }

        // Matrix / Scalar: component-wise division
        Div => {
            if lhs_ty.is_matrix() && rhs_ty.is_scalar() {
                let scalar = rhs_vals[0];
                let scalar_float = coercion::coerce_to_type(ctx, scalar, rhs_ty, &GlslType::Float)?;
                let mut result_vals = Vec::new();
                for &lhs_val in &lhs_vals {
                    result_vals.push(ctx.builder.ins().fdiv(lhs_val, scalar_float));
                }
                return Ok((result_vals, result_ty));
            }
            Err(GlslError::new(
                ErrorCode::E0106,
                "matrix division only supports matrix / scalar",
            )
            .with_location(source_span_to_location(&span)))
        }

        // Matrix == Matrix and Matrix != Matrix: aggregate comparison
        Equal | NonEqual => {
            if lhs_ty.is_matrix() && rhs_ty.is_matrix() {
                if lhs_ty != rhs_ty {
                    return Err(GlslError::new(
                        ErrorCode::E0106,
                        "matrix equality requires matching types",
                    )
                    .with_location(source_span_to_location(&span)));
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

                for i in 0..lhs_vals.len() {
                    let lhs_comp = lhs_vals[i];
                    let rhs_comp = rhs_vals[i];
                    // Compare components (returns I1) - matrices are always float-based
                    let cmp = ctx.builder.ins().fcmp(
                        cranelift_codegen::ir::condcodes::FloatCC::Equal,
                        lhs_comp,
                        rhs_comp,
                    );
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
            Err(GlslError::new(
                ErrorCode::E0106,
                "matrix equality requires both operands to be matrices",
            )
            .with_location(source_span_to_location(&span)))
        }

        _ => Err(GlslError::new(
            ErrorCode::E0106,
            format!("operator {op:?} not supported for matrices"),
        )
        .with_location(source_span_to_location(&span))),
    }
}
