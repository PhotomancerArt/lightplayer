//! Helper functions for code generation

use crate::error::{ErrorCode, GlslError};
use crate::frontend::codegen::context::CodegenContext;
use crate::semantic::types::Type;
use cranelift_codegen::ir::{ArgumentPurpose, InstBuilder, MemFlags, types};

use alloc::{format, vec::Vec};

/// Generate default return statement for a function
/// Used when function doesn't have explicit return
pub fn generate_default_return<M: cranelift_module::Module>(
    ctx: &mut CodegenContext<'_, M>,
    return_type: &Type,
) -> Result<(), GlslError> {
    if return_type == &Type::Void {
        ctx.builder.ins().return_(&[]);
        return Ok(());
    }

    if return_type.is_vector() {
        generate_default_vector_return(ctx, return_type)
    } else if return_type.is_matrix() {
        generate_default_matrix_return(ctx, return_type)
    } else {
        generate_default_scalar_return(ctx, return_type)
    }
}

fn generate_default_scalar_return<M: cranelift_module::Module>(
    ctx: &mut CodegenContext<'_, M>,
    return_type: &Type,
) -> Result<(), GlslError> {
    let return_val = match return_type {
        Type::Int | Type::UInt => ctx.builder.ins().iconst(types::I32, 0),
        Type::Float => ctx.builder.ins().f32const(0.0),
        Type::Bool => ctx.builder.ins().iconst(types::I8, 0),
        _ => {
            return Err(GlslError::new(
                ErrorCode::E0400,
                format!("unsupported return type for default return: {return_type:?}"),
            ));
        }
    };
    ctx.builder.ins().return_(&[return_val]);
    Ok(())
}

/// Helper: Get the StructReturn pointer from the function signature.
/// Returns an error if StructReturn is expected but not found.
fn get_structreturn_pointer<M: cranelift_module::Module>(
    ctx: &mut CodegenContext<'_, M>,
) -> Result<cranelift_codegen::ir::Value, GlslError> {
    ctx.builder
        .func
        .special_param(ArgumentPurpose::StructReturn)
        .ok_or_else(|| {
            GlslError::new(
                ErrorCode::E0400,
                "StructReturn parameter not found (internal error)",
            )
        })
}

/// Helper: Write zeros to StructReturn buffer for float elements.
/// Used for matrices and float-based vectors.
fn write_zeros_to_structreturn_buffer<M: cranelift_module::Module>(
    ctx: &mut CodegenContext<'_, M>,
    struct_ret_ptr: cranelift_codegen::ir::Value,
    element_count: usize,
) {
    for i in 0..element_count {
        let zero_val = ctx.builder.ins().f32const(0.0);
        let offset = (i * crate::frontend::codegen::constants::F32_SIZE_BYTES) as i32;
        ctx.builder
            .ins()
            .store(MemFlags::trusted(), zero_val, struct_ret_ptr, offset);
    }
}

/// Helper: Create a zero value for a given base type.
fn create_zero_value<M: cranelift_module::Module>(
    ctx: &mut CodegenContext<'_, M>,
    base_ty: &Type,
) -> Result<cranelift_codegen::ir::Value, GlslError> {
    match base_ty {
        Type::Float => Ok(ctx.builder.ins().f32const(0.0)),
        Type::Int | Type::UInt => Ok(ctx.builder.ins().iconst(types::I32, 0)),
        Type::Bool => Ok(ctx.builder.ins().iconst(types::I8, 0)),
        _ => Err(GlslError::new(
            ErrorCode::E0400,
            format!("unsupported base type for zero value: {base_ty:?}"),
        )),
    }
}

fn generate_default_vector_return<M: cranelift_module::Module>(
    ctx: &mut CodegenContext<'_, M>,
    return_type: &Type,
) -> Result<(), GlslError> {
    let uses_struct_return = ctx
        .builder
        .func
        .signature
        .uses_special_param(ArgumentPurpose::StructReturn);

    let base_ty = return_type.vector_base_type().ok_or_else(|| {
        GlslError::new(
            ErrorCode::E0400,
            format!("expected vector type, got: {return_type:?}"),
        )
    })?;
    let count = return_type.component_count().unwrap();

    if uses_struct_return {
        // Write zeros to StructReturn buffer
        let struct_ret_ptr = get_structreturn_pointer(ctx)?;

        // For float vectors, use optimized helper
        match base_ty {
            Type::Float => {
                write_zeros_to_structreturn_buffer(ctx, struct_ret_ptr, count);
            }
            _ => {
                // For int/bool vectors, write each component individually
                for i in 0..count {
                    let zero_val = create_zero_value(ctx, &base_ty)?;
                    let offset = (i * crate::frontend::codegen::constants::F32_SIZE_BYTES) as i32;
                    ctx.builder
                        .ins()
                        .store(MemFlags::trusted(), zero_val, struct_ret_ptr, offset);
                }
            }
        }

        // Return void for StructReturn functions
        ctx.builder.ins().return_(&[]);
    } else {
        // Legacy path (shouldn't happen with this plan, but kept as fallback)
        let mut vals = Vec::new();
        for _ in 0..count {
            let val = create_zero_value(ctx, &base_ty)?;
            vals.push(val);
        }
        ctx.builder.ins().return_(&vals);
    }
    Ok(())
}

fn generate_default_matrix_return<M: cranelift_module::Module>(
    ctx: &mut CodegenContext<'_, M>,
    return_type: &Type,
) -> Result<(), GlslError> {
    // Check if function uses StructReturn
    let uses_struct_return = ctx
        .builder
        .func
        .signature
        .uses_special_param(ArgumentPurpose::StructReturn);

    let element_count = return_type.matrix_element_count().ok_or_else(|| {
        GlslError::new(
            ErrorCode::E0400,
            format!("expected matrix type, got: {return_type:?}"),
        )
    })?;

    if uses_struct_return {
        // Write zeros to StructReturn buffer
        let struct_ret_ptr = get_structreturn_pointer(ctx)?;
        // Matrices are always float-based, use optimized helper
        write_zeros_to_structreturn_buffer(ctx, struct_ret_ptr, element_count);
        // Return void for StructReturn functions
        ctx.builder.ins().return_(&[]);
    } else {
        // Legacy path (shouldn't happen with this plan, but kept as fallback)
        let mut vals = Vec::new();
        // Matrices are always float-based, return zero matrix
        for _ in 0..element_count {
            vals.push(ctx.builder.ins().f32const(0.0));
        }
        ctx.builder.ins().return_(&vals);
    }
    Ok(())
}
