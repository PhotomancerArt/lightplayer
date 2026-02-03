use glsl::syntax::Expr;

use alloc::vec::Vec;

use crate::error::GlslError;
use crate::frontend::codegen::context::CodegenContext;

/// Emit return statement
pub fn emit_return_stmt<M: cranelift_module::Module>(
    ctx: &mut CodegenContext<'_, M>,
    expr: Option<&Expr>,
) -> Result<(), GlslError> {
    use crate::error::extract_span_from_expr;
    use cranelift_codegen::ir::{ArgumentPurpose, InstBuilder, MemFlags};

    if let Some(ret_expr) = expr {
        let span = extract_span_from_expr(ret_expr);
        let (ret_vals, ret_ty) = ctx.emit_expr_typed(ret_expr)?;
        log::trace!(
            "return statement: ret_ty={:?}, ret_vals.len()={}",
            ret_ty,
            ret_vals.len()
        );

        // Validate return type matches function signature
        if let Some(expected_ty) = &ctx.return_type {
            log::trace!("  expected_ty={expected_ty:?}");
            // Check if function uses StructReturn
            let uses_struct_return = ctx
                .builder
                .func
                .signature
                .uses_special_param(ArgumentPurpose::StructReturn);

            if uses_struct_return {
                // Function uses StructReturn - write values to buffer
                // Use special_param() method (like cranelift-examples) to get the StructReturn pointer
                let struct_ret_ptr = ctx
                    .builder
                    .func
                    .special_param(ArgumentPurpose::StructReturn)
                    .ok_or_else(|| {
                        GlslError::new(
                            crate::error::ErrorCode::E0400,
                            "StructReturn parameter not found (internal error)",
                        )
                    })?;

                // Coerce and write values to buffer at offsets (4 bytes per f32)
                let expected_base = if expected_ty.is_vector() {
                    expected_ty.vector_base_type().unwrap()
                } else {
                    crate::frontend::semantic::types::Type::Float
                };
                let ret_base = if ret_ty.is_vector() {
                    ret_ty.vector_base_type().unwrap()
                } else if ret_ty.is_matrix() {
                    crate::frontend::semantic::types::Type::Float
                } else {
                    ret_ty.clone()
                };

                log::trace!(
                    "  StructReturn: coercing {} values from {:?} to {:?}",
                    ret_vals.len(),
                    ret_base,
                    expected_base
                );
                for (i, val) in ret_vals.iter().enumerate() {
                    log::trace!(
                        "    processing element {i}: val={val:?}, val type should match ret_base={ret_base:?}"
                    );
                    let coerced = if ret_base == expected_base {
                        log::trace!("      no coercion needed for element {i}");
                        *val
                    } else {
                        log::trace!(
                            "      coercing element {i}: {ret_base:?} -> {expected_base:?}, val={val:?}"
                        );
                        ctx.coerce_to_type_with_location(
                            *val,
                            &ret_base,
                            &expected_base,
                            Some(span.clone()),
                        )?
                    };
                    log::trace!("      coerced value for element {i}: {coerced:?}");
                    let offset = (i * crate::frontend::codegen::constants::F32_SIZE_BYTES) as i32;
                    log::trace!("      storing coerced value at offset {offset}");
                    ctx.builder
                        .ins()
                        .store(MemFlags::trusted(), coerced, struct_ret_ptr, offset);
                }

                // Return void for StructReturn functions
                ctx.builder.ins().return_(&[]);
            } else if expected_ty.is_vector() || expected_ty.is_matrix() {
                // For vectors/matrices without StructReturn (shouldn't happen with this plan)
                // Keep existing behavior as fallback
                let expected_base = if expected_ty.is_vector() {
                    expected_ty.vector_base_type().unwrap()
                } else {
                    crate::frontend::semantic::types::Type::Float
                };
                let ret_base = if ret_ty.is_vector() {
                    ret_ty.vector_base_type().unwrap()
                } else if ret_ty.is_matrix() {
                    crate::frontend::semantic::types::Type::Float
                } else {
                    ret_ty.clone()
                };

                let mut coerced_vals = Vec::new();
                for val in ret_vals {
                    let coerced = if ret_base == expected_base {
                        val
                    } else {
                        ctx.coerce_to_type_with_location(
                            val,
                            &ret_base,
                            &expected_base,
                            Some(span.clone()),
                        )?
                    };
                    coerced_vals.push(coerced);
                }
                ctx.builder.ins().return_(&coerced_vals);
            } else {
                // For scalars, return single value with coercion if needed
                let expected_base = expected_ty.clone();
                let ret_base = ret_ty.clone();
                log::trace!(
                    "  scalar return: ret_base={ret_base:?}, expected_base={expected_base:?}"
                );

                let return_val = if ret_base == expected_base {
                    log::trace!("  types match, no coercion");
                    ret_vals[0]
                } else {
                    log::trace!("  coercing return value: {ret_base:?} -> {expected_base:?}");
                    ctx.coerce_to_type_with_location(
                        ret_vals[0],
                        &ret_base,
                        &expected_base,
                        Some(span.clone()),
                    )?
                };
                ctx.builder.ins().return_(&[return_val]);
            }
        } else {
            // No return type specified, use first value as-is
            ctx.builder.ins().return_(&[ret_vals[0]]);
        }
    } else {
        // Void return - return empty
        ctx.builder.ins().return_(&[]);
    }

    // Create unreachable block for subsequent code
    let unreachable = ctx.builder.create_block();
    ctx.emit_block(unreachable);

    Ok(())
}
