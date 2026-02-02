//! Write operations for LValue

use crate::error::{ErrorCode, GlslError};
use crate::frontend::codegen::context::CodegenContext;
use cranelift_codegen::ir::{InstBuilder, Value};

use alloc::format;

use super::super::expr::component;
use super::types::{LValue, PointerAccessPattern};

/// Write new value(s) to an LValue
///
/// Validates that the number of values matches the LValue's component count.
pub fn write_lvalue<M: cranelift_module::Module>(
    ctx: &mut CodegenContext<'_, M>,
    lvalue: &LValue,
    values: &[Value],
) -> Result<(), GlslError> {
    // Must be in block to write variables
    ctx.ensure_block()?;

    match lvalue {
        LValue::PointerBased {
            ptr,
            base_ty,
            access_pattern,
        } => {
            let flags = cranelift_codegen::ir::MemFlags::trusted();
            match access_pattern {
                PointerAccessPattern::Direct { component_count } => {
                    // Store all components
                    if values.len() != *component_count {
                        return Err(GlslError::new(
                            ErrorCode::E0400,
                            format!(
                                "component count mismatch: expected {}, got {}",
                                component_count,
                                values.len()
                            ),
                        ));
                    }
                    let base_cranelift_ty = if base_ty.is_vector() {
                        base_ty
                            .vector_base_type()
                            .unwrap()
                            .to_cranelift_type()
                            .map_err(|e| {
                                GlslError::new(
                                    ErrorCode::E0400,
                                    format!("Failed to convert type: {}", e.message),
                                )
                            })?
                    } else if base_ty.is_matrix() {
                        cranelift_codegen::ir::types::F32
                    } else {
                        base_ty.to_cranelift_type().map_err(|e| {
                            GlslError::new(
                                ErrorCode::E0400,
                                format!("Failed to convert type: {}", e.message),
                            )
                        })?
                    };
                    let component_size_bytes = base_cranelift_ty.bytes() as usize;
                    for (i, &val) in values.iter().enumerate() {
                        let offset = (i * component_size_bytes) as i32;
                        ctx.builder.ins().store(flags, val, *ptr, offset);
                    }
                    Ok(())
                }
                PointerAccessPattern::Component { indices, .. } => {
                    // Store only requested components
                    if values.len() != indices.len() {
                        return Err(GlslError::new(
                            ErrorCode::E0400,
                            format!(
                                "component count mismatch: {} indices, {} values",
                                indices.len(),
                                values.len()
                            ),
                        ));
                    }
                    let base_cranelift_ty = base_ty
                        .vector_base_type()
                        .unwrap()
                        .to_cranelift_type()
                        .map_err(|e| {
                            GlslError::new(
                                ErrorCode::E0400,
                                format!("Failed to convert type: {}", e.message),
                            )
                        })?;
                    let component_size_bytes = base_cranelift_ty.bytes() as usize;
                    for (idx, val) in indices.iter().zip(values.iter()) {
                        let offset = (*idx * component_size_bytes) as i32;
                        ctx.builder.ins().store(flags, *val, *ptr, offset);
                    }
                    Ok(())
                }
                PointerAccessPattern::ArrayElement {
                    index,
                    index_val,
                    element_ty,
                    element_size_bytes,
                    component_indices,
                } => {
                    // Calculate element offset
                    let (final_ptr, base_offset) = if let Some(compile_idx) = index {
                        // Compile-time constant offset
                        let offset = (compile_idx * element_size_bytes) as i32;
                        (*ptr, offset)
                    } else if let Some(runtime_idx) = index_val {
                        // Runtime offset calculation
                        let element_size_const = ctx.builder.ins().iconst(
                            cranelift_codegen::ir::types::I32,
                            *element_size_bytes as i64,
                        );
                        let offset_val = ctx.builder.ins().imul(*runtime_idx, element_size_const);
                        let pointer_type = ctx.gl_module.module_internal().isa().pointer_type();
                        let offset_for_ptr = if pointer_type == cranelift_codegen::ir::types::I32 {
                            offset_val
                        } else {
                            ctx.builder.ins().uextend(pointer_type, offset_val)
                        };
                        let final_ptr = ctx.builder.ins().iadd(*ptr, offset_for_ptr);
                        (final_ptr, 0)
                    } else {
                        return Err(GlslError::new(
                            ErrorCode::E0400,
                            "array element access requires index",
                        ));
                    };

                    // Get base Cranelift type for storing
                    let base_cranelift_ty = if element_ty.is_vector() {
                        let base_ty = element_ty.vector_base_type().unwrap();
                        base_ty.to_cranelift_type().map_err(|e| {
                            GlslError::new(
                                ErrorCode::E0400,
                                format!("Failed to convert vector base type: {}", e.message),
                            )
                        })?
                    } else if element_ty.is_matrix() {
                        cranelift_codegen::ir::types::F32
                    } else {
                        element_ty.to_cranelift_type().map_err(|e| {
                            GlslError::new(
                                ErrorCode::E0400,
                                format!("Failed to convert element type: {}", e.message),
                            )
                        })?
                    };
                    let component_size_bytes = base_cranelift_ty.bytes() as usize;

                    // Handle component access (e.g., arr[i].x = value)
                    if let Some(component_indices) = component_indices {
                        if !element_ty.is_vector() {
                            return Err(GlslError::new(
                                ErrorCode::E0400,
                                "component access only supported for vector array elements",
                            ));
                        }
                        if component_indices.len() != values.len() {
                            return Err(GlslError::new(
                                ErrorCode::E0400,
                                format!(
                                    "component count mismatch: {} indices, {} values",
                                    component_indices.len(),
                                    values.len()
                                ),
                            ));
                        }
                        for (&comp_idx, &val) in component_indices.iter().zip(values.iter()) {
                            let component_offset = (comp_idx * component_size_bytes) as i32;
                            let total_offset = base_offset + component_offset;
                            ctx.builder.ins().store(flags, val, final_ptr, total_offset);
                        }
                        Ok(())
                    } else {
                        // Store entire element
                        if element_ty.is_scalar() {
                            if values.len() != 1 {
                                return Err(GlslError::new(
                                    ErrorCode::E0400,
                                    format!(
                                        "scalar array element requires 1 value, got {}",
                                        values.len()
                                    ),
                                ));
                            }
                            ctx.builder
                                .ins()
                                .store(flags, values[0], final_ptr, base_offset);
                            Ok(())
                        } else if element_ty.is_vector() {
                            let component_count = element_ty.component_count().unwrap();
                            if values.len() != component_count {
                                return Err(GlslError::new(
                                    ErrorCode::E0400,
                                    format!(
                                        "vector array element requires {} values, got {}",
                                        component_count,
                                        values.len()
                                    ),
                                ));
                            }
                            for (i, &val) in values.iter().enumerate() {
                                let component_offset = (i * component_size_bytes) as i32;
                                let total_offset = base_offset + component_offset;
                                ctx.builder.ins().store(flags, val, final_ptr, total_offset);
                            }
                            Ok(())
                        } else if element_ty.is_matrix() {
                            let component_count = element_ty.matrix_element_count().unwrap();
                            if values.len() != component_count {
                                return Err(GlslError::new(
                                    ErrorCode::E0400,
                                    format!(
                                        "matrix array element requires {} values, got {}",
                                        component_count,
                                        values.len()
                                    ),
                                ));
                            }
                            for (i, &val) in values.iter().enumerate() {
                                let component_offset = (i * component_size_bytes) as i32;
                                let total_offset = base_offset + component_offset;
                                ctx.builder.ins().store(flags, val, final_ptr, total_offset);
                            }
                            Ok(())
                        } else {
                            Err(GlslError::new(
                                ErrorCode::E0400,
                                format!("unsupported array element type: {element_ty:?}"),
                            ))
                        }
                    }
                }
            }
        }

        LValue::Variable { vars, .. } => {
            // Normal variable: use vars
            // Out/inout parameters now use PointerBased variant
            if vars.len() != values.len() {
                return Err(GlslError::new(
                    ErrorCode::E0400,
                    format!(
                        "component count mismatch: {} variables, {} values",
                        vars.len(),
                        values.len()
                    ),
                ));
            }
            for (var, val) in vars.iter().zip(values.iter()) {
                ctx.builder.def_var(*var, *val);
            }
            Ok(())
        }

        LValue::Component {
            base_vars, indices, ..
        } => {
            // Normal component access: use vars
            // Out/inout component access now uses PointerBased variant
            if indices.len() != values.len() {
                return Err(GlslError::new(
                    ErrorCode::E0400,
                    format!(
                        "component count mismatch: {} indices, {} values",
                        indices.len(),
                        values.len()
                    ),
                ));
            }
            for (&idx, &val) in indices.iter().zip(values.iter()) {
                ctx.builder.def_var(base_vars[idx], val);
            }
            Ok(())
        }

        LValue::MatrixElement {
            base_vars,
            base_ty,
            row,
            col,
        } => {
            if values.len() != 1 {
                return Err(GlslError::new(
                    ErrorCode::E0400,
                    format!("matrix element requires 1 value, got {}", values.len()),
                ));
            }
            let (rows, _cols) = base_ty.matrix_dims().unwrap();
            ctx.store_matrix_element(base_vars, *col, *row, rows, values[0]);
            Ok(())
        }

        LValue::MatrixColumn {
            base_vars,
            base_ty,
            col,
            result_ty,
        } => {
            let (rows, _cols) = base_ty.matrix_dims().unwrap();
            let expected_count = result_ty.component_count().unwrap();
            if values.len() != expected_count {
                return Err(GlslError::new(
                    ErrorCode::E0400,
                    format!(
                        "matrix column requires {} values, got {}",
                        expected_count,
                        values.len()
                    ),
                ));
            }
            for (row_idx, &val) in values.iter().enumerate() {
                ctx.store_matrix_element(base_vars, *col, row_idx, rows, val);
            }
            Ok(())
        }

        LValue::VectorElement {
            base_vars, index, ..
        } => {
            if values.len() != 1 {
                return Err(GlslError::new(
                    ErrorCode::E0400,
                    format!("vector element requires 1 value, got {}", values.len()),
                ));
            }
            ctx.builder.def_var(base_vars[*index], values[0]);
            Ok(())
        }

        LValue::ArrayElement {
            array_ptr,
            base_ty,
            element_ty,
            index,
            index_val,
            element_size_bytes,
            component_indices,
            ..
        } => {
            // Emit bounds check for runtime indices (compile-time constants are already validated)
            if let Some(runtime_idx) = index_val {
                let array_size = base_ty.array_dimensions()[0];
                // Use unknown span for error reporting (runtime checks don't have exact span)
                let dummy_span = glsl::syntax::SourceSpan::unknown();
                component::emit_bounds_check(ctx, *runtime_idx, array_size, &dummy_span)?;
            }

            // Calculate byte offset and final pointer
            // For runtime offsets, add offset to pointer and use offset 0
            let (final_ptr, base_offset) = if let Some(compile_idx) = index {
                // Compile-time constant offset - can use directly
                let offset = (compile_idx * element_size_bytes) as i32;
                (*array_ptr, offset)
            } else if let Some(runtime_idx) = index_val {
                // Runtime offset calculation - add to pointer
                let element_size_const = ctx.builder.ins().iconst(
                    cranelift_codegen::ir::types::I32,
                    *element_size_bytes as i64,
                );
                let offset_val = ctx.builder.ins().imul(*runtime_idx, element_size_const);
                let pointer_type = ctx.gl_module.module_internal().isa().pointer_type();
                // If pointer type matches offset type, use offset directly; otherwise extend
                let offset_for_ptr = if pointer_type == cranelift_codegen::ir::types::I32 {
                    offset_val
                } else {
                    ctx.builder.ins().uextend(pointer_type, offset_val)
                };
                let final_ptr = ctx.builder.ins().iadd(*array_ptr, offset_for_ptr);
                (final_ptr, 0)
            } else {
                return Err(GlslError::new(
                    ErrorCode::E0400,
                    "array element access requires index",
                ));
            };

            // Get base Cranelift type for storing (scalar component type)
            let base_cranelift_ty = if element_ty.is_vector() {
                let base_ty = element_ty.vector_base_type().unwrap();
                base_ty.to_cranelift_type().map_err(|e| {
                    GlslError::new(
                        ErrorCode::E0400,
                        format!("Failed to convert vector base type: {}", e.message),
                    )
                })?
            } else if element_ty.is_matrix() {
                // Matrices are always float
                cranelift_codegen::ir::types::F32
            } else {
                // Scalar
                element_ty.to_cranelift_type().map_err(|e| {
                    GlslError::new(
                        ErrorCode::E0400,
                        format!("Failed to convert element type: {}", e.message),
                    )
                })?
            };

            // Calculate component size (base type size)
            let component_size_bytes = base_cranelift_ty.bytes() as usize;

            let flags = cranelift_codegen::ir::MemFlags::trusted();

            // Handle component access (e.g., arr[i].x = value)
            if let Some(component_indices) = component_indices {
                if !element_ty.is_vector() {
                    return Err(GlslError::new(
                        ErrorCode::E0400,
                        "component access only supported for vector array elements",
                    ));
                }

                if component_indices.len() != values.len() {
                    return Err(GlslError::new(
                        ErrorCode::E0400,
                        format!(
                            "component count mismatch: {} indices, {} values",
                            component_indices.len(),
                            values.len()
                        ),
                    ));
                }

                log::trace!(
                    "write_lvalue ArrayElement with component access: element_ty={element_ty:?}, component_indices={component_indices:?}, base_offset={base_offset}, component_size_bytes={component_size_bytes}"
                );

                for (&comp_idx, &val) in component_indices.iter().zip(values.iter()) {
                    let component_offset = (comp_idx * component_size_bytes) as i32;
                    let total_offset = base_offset + component_offset;
                    log::trace!(
                        "  Storing component {comp_idx}: comp_idx={comp_idx}, component_offset={component_offset}, total_offset={total_offset}"
                    );
                    ctx.builder.ins().store(flags, val, final_ptr, total_offset);
                }

                Ok(())
            } else {
                // Store entire element
                if element_ty.is_scalar() {
                    // Single scalar value
                    if values.len() != 1 {
                        return Err(GlslError::new(
                            ErrorCode::E0400,
                            format!(
                                "scalar array element requires 1 value, got {}",
                                values.len()
                            ),
                        ));
                    }
                    ctx.builder
                        .ins()
                        .store(flags, values[0], final_ptr, base_offset);
                    Ok(())
                } else if element_ty.is_vector() {
                    // Multi-component element - store each component
                    let component_count = element_ty.component_count().unwrap();
                    if values.len() != component_count {
                        return Err(GlslError::new(
                            ErrorCode::E0400,
                            format!(
                                "vector array element requires {} values, got {}",
                                component_count,
                                values.len()
                            ),
                        ));
                    }

                    for (i, &val) in values.iter().enumerate() {
                        let component_offset = (i * component_size_bytes) as i32;
                        let total_offset = base_offset + component_offset;
                        ctx.builder.ins().store(flags, val, final_ptr, total_offset);
                    }
                    Ok(())
                } else if element_ty.is_matrix() {
                    // Multi-component element - store each component
                    let component_count = element_ty.matrix_element_count().unwrap();
                    if values.len() != component_count {
                        return Err(GlslError::new(
                            ErrorCode::E0400,
                            format!(
                                "matrix array element requires {} values, got {}",
                                component_count,
                                values.len()
                            ),
                        ));
                    }

                    for (i, &val) in values.iter().enumerate() {
                        let component_offset = (i * component_size_bytes) as i32;
                        let total_offset = base_offset + component_offset;
                        ctx.builder.ins().store(flags, val, final_ptr, total_offset);
                    }
                    Ok(())
                } else {
                    Err(GlslError::new(
                        ErrorCode::E0400,
                        format!("unsupported array element type: {element_ty:?}"),
                    ))
                }
            }
        }
    }
}
