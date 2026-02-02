//! Read operations for LValue

use crate::error::{ErrorCode, GlslError};
use crate::frontend::codegen::context::CodegenContext;
use crate::semantic::types::Type as GlslType;
use alloc::{format, vec, vec::Vec};
use cranelift_codegen::ir::{InstBuilder, Value};

use super::super::expr::component;
use super::types::{LValue, PointerAccessPattern};

/// Read the current value(s) from an LValue
///
/// Returns the values and their type.
pub fn read_lvalue<M: cranelift_module::Module>(
    ctx: &mut CodegenContext<'_, M>,
    lvalue: &LValue,
) -> Result<(Vec<Value>, GlslType), GlslError> {
    // Must be in block to read variables
    ctx.ensure_block()?;

    match lvalue {
        LValue::Variable { vars, ty, .. } => {
            // Normal variable or array: use vars (arrays have empty vars)
            // Out/inout parameters now use PointerBased variant
            let vals: Vec<Value> = vars.iter().map(|&v| ctx.builder.use_var(v)).collect();
            Ok((vals, ty.clone()))
        }

        LValue::Component {
            base_vars,
            indices,
            result_ty,
            ..
        } => {
            // Normal component access: use vars
            // Out/inout component access now uses PointerBased variant
            let mut vals = Vec::new();
            for &idx in indices {
                vals.push(ctx.builder.use_var(base_vars[idx]));
            }
            Ok((vals, result_ty.clone()))
        }

        LValue::MatrixElement {
            base_vars,
            base_ty,
            row,
            col,
        } => {
            let (rows, _cols) = base_ty.matrix_dims().unwrap();
            let val = ctx.load_matrix_element(base_vars, *col, *row, rows);
            Ok((vec![val], GlslType::Float)) // Matrix elements are always float
        }

        LValue::MatrixColumn {
            base_vars,
            base_ty,
            col,
            result_ty,
        } => {
            let (rows, _cols) = base_ty.matrix_dims().unwrap();
            let vals = ctx.load_matrix_column(base_vars, *col, rows);
            Ok((vals, result_ty.clone()))
        }

        LValue::VectorElement {
            base_vars,
            base_ty,
            index,
        } => {
            let val = ctx.builder.use_var(base_vars[*index]);
            let base_type = base_ty.vector_base_type().unwrap();
            log::trace!(
                "read_lvalue VectorElement: base_ty={base_ty:?}, base_type={base_type:?}, index={index}"
            );
            Ok((vec![val], base_type))
        }

        LValue::PointerBased {
            ptr,
            base_ty,
            access_pattern,
        } => {
            let flags = cranelift_codegen::ir::MemFlags::trusted();
            match access_pattern {
                PointerAccessPattern::Direct { component_count } => {
                    // Load all components
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
                    let mut vals = Vec::new();
                    for i in 0..*component_count {
                        let offset = (i * component_size_bytes) as i32;
                        let val = ctx
                            .builder
                            .ins()
                            .load(base_cranelift_ty, flags, *ptr, offset);
                        vals.push(val);
                    }
                    Ok((vals, base_ty.clone()))
                }
                PointerAccessPattern::Component { indices, result_ty } => {
                    // Load only requested components
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
                    let mut vals = Vec::new();
                    for &idx in indices {
                        let offset = (idx * component_size_bytes) as i32;
                        let val = ctx
                            .builder
                            .ins()
                            .load(base_cranelift_ty, flags, *ptr, offset);
                        vals.push(val);
                    }
                    Ok((vals, result_ty.clone()))
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

                    // Get base Cranelift type for loading
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

                    // Handle component access (e.g., arr[i].x)
                    if let Some(component_indices) = component_indices {
                        if !element_ty.is_vector() {
                            return Err(GlslError::new(
                                ErrorCode::E0400,
                                "component access only supported for vector array elements",
                            ));
                        }
                        let mut vals = Vec::new();
                        for &comp_idx in component_indices {
                            let component_offset = (comp_idx * component_size_bytes) as i32;
                            let total_offset = base_offset + component_offset;
                            let val = ctx.builder.ins().load(
                                base_cranelift_ty,
                                flags,
                                final_ptr,
                                total_offset,
                            );
                            vals.push(val);
                        }
                        let result_ty = if component_indices.len() == 1 {
                            element_ty.vector_base_type().unwrap()
                        } else {
                            element_ty
                                .vector_base_type()
                                .and_then(|base| {
                                    GlslType::vector_type(&base, component_indices.len())
                                })
                                .unwrap_or(element_ty.clone())
                        };
                        Ok((vals, result_ty))
                    } else {
                        // Load entire element
                        if element_ty.is_scalar() {
                            let val = ctx.builder.ins().load(
                                base_cranelift_ty,
                                flags,
                                final_ptr,
                                base_offset,
                            );
                            Ok((vec![val], element_ty.clone()))
                        } else if element_ty.is_vector() {
                            let component_count = element_ty.component_count().unwrap();
                            let mut vals = Vec::new();
                            for i in 0..component_count {
                                let component_offset = (i * component_size_bytes) as i32;
                                let total_offset = base_offset + component_offset;
                                let val = ctx.builder.ins().load(
                                    base_cranelift_ty,
                                    flags,
                                    final_ptr,
                                    total_offset,
                                );
                                vals.push(val);
                            }
                            Ok((vals, element_ty.clone()))
                        } else if element_ty.is_matrix() {
                            let component_count = element_ty.matrix_element_count().unwrap();
                            let mut vals = Vec::new();
                            for i in 0..component_count {
                                let component_offset = (i * component_size_bytes) as i32;
                                let total_offset = base_offset + component_offset;
                                let val = ctx.builder.ins().load(
                                    base_cranelift_ty,
                                    flags,
                                    final_ptr,
                                    total_offset,
                                );
                                vals.push(val);
                            }
                            Ok((vals, element_ty.clone()))
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

            // Get base Cranelift type for loading (scalar component type)
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

            // Handle component access (e.g., arr[i].x)
            if let Some(component_indices) = component_indices {
                if !element_ty.is_vector() {
                    return Err(GlslError::new(
                        ErrorCode::E0400,
                        "component access only supported for vector array elements",
                    ));
                }

                log::trace!(
                    "read_lvalue ArrayElement with component access: element_ty={element_ty:?}, component_indices={component_indices:?}, base_offset={base_offset}, component_size_bytes={component_size_bytes}, final_ptr={final_ptr:?}"
                );

                let mut vals = Vec::new();
                for &comp_idx in component_indices {
                    let component_offset = (comp_idx * component_size_bytes) as i32;
                    let total_offset = base_offset + component_offset;
                    log::trace!(
                        "  Loading component {comp_idx}: comp_idx={comp_idx}, component_offset={component_offset}, total_offset={total_offset}"
                    );
                    let val =
                        ctx.builder
                            .ins()
                            .load(base_cranelift_ty, flags, final_ptr, total_offset);
                    log::trace!("  Loaded value: {val:?}");
                    vals.push(val);
                }

                let result_ty = if component_indices.len() == 1 {
                    element_ty.vector_base_type().unwrap()
                } else {
                    element_ty
                        .vector_base_type()
                        .and_then(|base| GlslType::vector_type(&base, component_indices.len()))
                        .unwrap_or(element_ty.clone())
                };

                Ok((vals, result_ty))
            } else {
                // Load entire element
                if element_ty.is_scalar() {
                    // Single scalar value
                    let val =
                        ctx.builder
                            .ins()
                            .load(base_cranelift_ty, flags, final_ptr, base_offset);
                    Ok((vec![val], element_ty.clone()))
                } else if element_ty.is_vector() {
                    // Multi-component element - load each component
                    let component_count = element_ty.component_count().unwrap();
                    let mut vals = Vec::new();
                    for i in 0..component_count {
                        let component_offset = (i * component_size_bytes) as i32;
                        let total_offset = base_offset + component_offset;
                        let val = ctx.builder.ins().load(
                            base_cranelift_ty,
                            flags,
                            final_ptr,
                            total_offset,
                        );
                        vals.push(val);
                    }
                    Ok((vals, element_ty.clone()))
                } else if element_ty.is_matrix() {
                    // Multi-component element - load each component
                    let component_count = element_ty.matrix_element_count().unwrap();
                    let mut vals = Vec::new();
                    for i in 0..component_count {
                        let component_offset = (i * component_size_bytes) as i32;
                        let total_offset = base_offset + component_offset;
                        let val = ctx.builder.ins().load(
                            base_cranelift_ty,
                            flags,
                            final_ptr,
                            total_offset,
                        );
                        vals.push(val);
                    }
                    Ok((vals, element_ty.clone()))
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
