use glsl::syntax::Declaration;

use alloc::{boxed::Box, format, vec::Vec};

use crate::error::{ErrorCode, GlslError};
use crate::frontend::codegen::context::CodegenContext;
use cranelift_codegen::ir::InstBuilder;

/// Emit variable declaration statement
pub fn emit_declaration<M: cranelift_module::Module>(
    ctx: &mut CodegenContext<'_, M>,
    decl: &Declaration,
) -> Result<(), GlslError> {
    use glsl::syntax::Declaration;

    match decl {
        Declaration::InitDeclaratorList(list) => {
            // Get base type from type specifier (for tail declarations)
            let base_ty = parse_type_specifier(ctx, &list.head.ty)?;

            // Handle the head declaration
            if let Some(name) = &list.head.name {
                // Check for unsized array and infer size from initializer if present
                let mut ty = crate::frontend::semantic::type_resolver::parse_head_declarator_type(
                    list, &name.span,
                )?;

                // Handle unsized arrays: infer size from initializer
                if ty.is_array() {
                    let array_dims = ty.array_dimensions();
                    // Check if first dimension is 0 (unsized)
                    if let Some(&first_dim) = array_dims.first() {
                        if first_dim == 0 {
                            // Unsized array - need initializer to infer size
                            if let Some(init) = &list.head.initializer {
                                let (init_vals, _init_ty) = emit_initializer(ctx, init)?;
                                let inferred_size = init_vals.len();

                                // Rebuild array type with inferred size
                                let element_ty = ty.array_element_type().unwrap();
                                ty = crate::frontend::semantic::types::Type::Array(
                                    Box::new(element_ty),
                                    inferred_size,
                                );
                            } else {
                                return Err(GlslError::new(
                                    ErrorCode::E0400,
                                    "unsized arrays require initializer to infer size",
                                ));
                            }
                        }
                    }
                }

                let vars = ctx.declare_variable(name.name.clone(), ty.clone())?;

                // Handle initializer if present
                if ty.is_array() {
                    if let Some(init) = &list.head.initializer {
                        // Ensure we're in a block before emitting instructions
                        ctx.ensure_block()?;

                        // Array initialization
                        let (init_vals, init_element_ty) = emit_initializer(ctx, init)?;

                        let element_ty = ty.array_element_type().unwrap();
                        let array_size = ty.array_dimensions()[0];

                        // Validate initializer list length
                        if init_vals.len() > array_size {
                            return Err(GlslError::new(
                                ErrorCode::E0400,
                                format!(
                                    "array initializer has {} elements, but array size is {}",
                                    init_vals.len(),
                                    array_size
                                ),
                            ));
                        }

                        // Type check element type
                        crate::frontend::semantic::type_check::check_assignment(
                            &element_ty,
                            &init_element_ty,
                        )
                        .map_err(|_e| {
                            GlslError::new(
                                ErrorCode::E0400,
                                format!(
                                    "array element type mismatch: expected {element_ty:?}, got {init_element_ty:?}"
                                ),
                            )
                        })?;

                        // Get array pointer
                        let var_info = ctx.lookup_var_info(&name.name).ok_or_else(|| {
                            GlslError::new(
                                ErrorCode::E0400,
                                format!("array variable '{}' not found", name.name),
                            )
                        })?;
                        let array_ptr = var_info.array_ptr.ok_or_else(|| {
                            GlslError::new(
                                ErrorCode::E0400,
                                format!("variable '{}' is not an array", name.name),
                            )
                        })?;

                        // Calculate element size
                        let element_size_bytes =
                            ctx.calculate_array_element_size_bytes(&element_ty)?;

                        // Coerce and store initializer values
                        let base_ty = element_ty.clone();
                        let flags = cranelift_codegen::ir::MemFlags::trusted();

                        for (index, init_val) in init_vals.iter().enumerate() {
                            let coerced_val =
                                ctx.coerce_to_type(*init_val, &init_element_ty, &base_ty)?;
                            let offset = (index * element_size_bytes) as i32;
                            ctx.builder
                                .ins()
                                .store(flags, coerced_val, array_ptr, offset);
                        }

                        // Zero-fill remaining elements for partial initialization
                        if init_vals.len() < array_size {
                            let zero_val = match element_ty {
                                crate::frontend::semantic::types::Type::Int => ctx
                                    .builder
                                    .ins()
                                    .iconst(cranelift_codegen::ir::types::I32, 0),
                                crate::frontend::semantic::types::Type::Float => {
                                    ctx.builder.ins().f32const(0.0)
                                }
                                crate::frontend::semantic::types::Type::Bool => ctx
                                    .builder
                                    .ins()
                                    .iconst(cranelift_codegen::ir::types::I32, 0),
                                _ => {
                                    return Err(GlslError::new(
                                        ErrorCode::E0400,
                                        format!(
                                            "unsupported array element type for zero initialization: {element_ty:?}"
                                        ),
                                    ));
                                }
                            };

                            for index in init_vals.len()..array_size {
                                let offset = (index * element_size_bytes) as i32;
                                ctx.builder.ins().store(flags, zero_val, array_ptr, offset);
                            }
                        }
                    }
                } else if let Some(init) = &list.head.initializer {
                    let (init_vals, init_ty) = emit_initializer(ctx, init)?;

                    // Type check (allows implicit conversions)
                    // Extract span from initializer for error reporting
                    let init_span = match init {
                        glsl::syntax::Initializer::Simple(expr) => {
                            crate::error::extract_span_from_expr(expr.as_ref())
                        }
                        _ => glsl::syntax::SourceSpan::unknown(),
                    };
                    match crate::frontend::semantic::type_check::check_assignment(&ty, &init_ty) {
                        Ok(()) => {}
                        Err(mut error) => {
                            if error.location.is_none() {
                                error = error.with_location(crate::error::source_span_to_location(
                                    &init_span,
                                ));
                            }
                            return Err(ctx.add_span_to_error(error, &init_span));
                        }
                    }

                    // Coerce initializer values to match variable type
                    let base_ty = if ty.is_vector() {
                        ty.vector_base_type().unwrap()
                    } else if ty.is_matrix() {
                        crate::frontend::semantic::types::Type::Float
                    } else {
                        ty.clone()
                    };
                    let init_base = if init_ty.is_vector() {
                        init_ty.vector_base_type().unwrap()
                    } else if init_ty.is_matrix() {
                        crate::frontend::semantic::types::Type::Float
                    } else {
                        init_ty.clone()
                    };

                    // Check component counts match
                    if vars.len() != init_vals.len() {
                        return Err(GlslError::new(
                            ErrorCode::E0400,
                            format!(
                                "component count mismatch: variable has {} components, initializer has {}",
                                vars.len(),
                                init_vals.len()
                            ),
                        ));
                    }

                    // Assign each component with type coercion
                    for (var, val) in vars.iter().zip(&init_vals) {
                        let coerced_val = ctx.coerce_to_type(*val, &init_base, &base_ty)?;
                        ctx.builder.def_var(*var, coerced_val);
                    }
                }
            }

            // Handle tail declarations (same type, different names)
            for declarator in &list.tail {
                // Parse complete type including array specifier from ArrayedIdentifier
                let mut declarator_ty =
                    crate::frontend::semantic::type_resolver::parse_tail_declarator_type(
                        &base_ty, declarator,
                    )?;

                // Handle unsized arrays: infer size from initializer
                if declarator_ty.is_array() {
                    let array_dims = declarator_ty.array_dimensions();
                    if let Some(&first_dim) = array_dims.first() {
                        if first_dim == 0 {
                            // Unsized array - need initializer to infer size
                            if let Some(init) = &declarator.initializer {
                                let (init_vals, _init_ty) = emit_initializer(ctx, init)?;
                                let inferred_size = init_vals.len();

                                // Rebuild array type with inferred size
                                let element_ty = declarator_ty.array_element_type().unwrap();
                                declarator_ty = crate::frontend::semantic::types::Type::Array(
                                    Box::new(element_ty),
                                    inferred_size,
                                );
                            } else {
                                return Err(GlslError::new(
                                    ErrorCode::E0400,
                                    "unsized arrays require initializer to infer size",
                                ));
                            }
                        }
                    }
                }

                let vars = ctx
                    .declare_variable(declarator.ident.ident.name.clone(), declarator_ty.clone())?;

                // Handle initializer if present
                if declarator_ty.is_array() {
                    if let Some(init) = &declarator.initializer {
                        // Ensure we're in a block before emitting instructions
                        ctx.ensure_block()?;

                        // Array initialization
                        let (init_vals, init_element_ty) = emit_initializer(ctx, init)?;

                        let element_ty = declarator_ty.array_element_type().unwrap();
                        let array_size = declarator_ty.array_dimensions()[0];

                        // Validate initializer list length
                        if init_vals.len() > array_size {
                            return Err(GlslError::new(
                                ErrorCode::E0400,
                                format!(
                                    "array initializer has {} elements, but array size is {}",
                                    init_vals.len(),
                                    array_size
                                ),
                            ));
                        }

                        // Type check element type
                        crate::frontend::semantic::type_check::check_assignment(
                            &element_ty,
                            &init_element_ty,
                        )
                        .map_err(|_e| {
                            GlslError::new(
                                ErrorCode::E0400,
                                format!(
                                    "array element type mismatch: expected {element_ty:?}, got {init_element_ty:?}"
                                ),
                            )
                        })?;

                        // Get array pointer
                        let var_info = ctx
                            .lookup_var_info(&declarator.ident.ident.name)
                            .ok_or_else(|| {
                                GlslError::new(
                                    ErrorCode::E0400,
                                    format!(
                                        "array variable '{}' not found",
                                        declarator.ident.ident.name
                                    ),
                                )
                            })?;
                        let array_ptr = var_info.array_ptr.ok_or_else(|| {
                            GlslError::new(
                                ErrorCode::E0400,
                                format!(
                                    "variable '{}' is not an array",
                                    declarator.ident.ident.name
                                ),
                            )
                        })?;

                        // Calculate element size
                        let element_size_bytes =
                            ctx.calculate_array_element_size_bytes(&element_ty)?;

                        // Coerce and store initializer values
                        let base_ty = element_ty.clone();
                        let flags = cranelift_codegen::ir::MemFlags::trusted();

                        for (index, init_val) in init_vals.iter().enumerate() {
                            let coerced_val =
                                ctx.coerce_to_type(*init_val, &init_element_ty, &base_ty)?;
                            let offset = (index * element_size_bytes) as i32;
                            ctx.builder
                                .ins()
                                .store(flags, coerced_val, array_ptr, offset);
                        }

                        // Zero-fill remaining elements for partial initialization
                        if init_vals.len() < array_size {
                            let zero_val = match element_ty {
                                crate::frontend::semantic::types::Type::Int => ctx
                                    .builder
                                    .ins()
                                    .iconst(cranelift_codegen::ir::types::I32, 0),
                                crate::frontend::semantic::types::Type::Float => {
                                    ctx.builder.ins().f32const(0.0)
                                }
                                crate::frontend::semantic::types::Type::Bool => ctx
                                    .builder
                                    .ins()
                                    .iconst(cranelift_codegen::ir::types::I32, 0),
                                _ => {
                                    return Err(GlslError::new(
                                        ErrorCode::E0400,
                                        format!(
                                            "unsupported array element type for zero initialization: {element_ty:?}"
                                        ),
                                    ));
                                }
                            };

                            for index in init_vals.len()..array_size {
                                let offset = (index * element_size_bytes) as i32;
                                ctx.builder.ins().store(flags, zero_val, array_ptr, offset);
                            }
                        }
                    }
                } else if let Some(init) = &declarator.initializer {
                    let (init_vals, init_ty) = emit_initializer(ctx, init)?;

                    // Type check (allows implicit conversions)
                    crate::frontend::semantic::type_check::check_assignment(
                        &declarator_ty,
                        &init_ty,
                    )?;

                    // Coerce initializer values to match variable type
                    let base_ty = if declarator_ty.is_vector() {
                        declarator_ty.vector_base_type().unwrap()
                    } else if declarator_ty.is_matrix() {
                        crate::frontend::semantic::types::Type::Float
                    } else {
                        declarator_ty.clone()
                    };
                    let init_base = if init_ty.is_vector() {
                        init_ty.vector_base_type().unwrap()
                    } else if init_ty.is_matrix() {
                        crate::frontend::semantic::types::Type::Float
                    } else {
                        init_ty.clone()
                    };

                    // Check component counts match
                    if vars.len() != init_vals.len() {
                        return Err(GlslError::new(
                            ErrorCode::E0400,
                            format!(
                                "component count mismatch: variable has {} components, initializer has {}",
                                vars.len(),
                                init_vals.len()
                            ),
                        ));
                    }

                    // Assign each component with type coercion
                    for (var, val) in vars.iter().zip(&init_vals) {
                        let coerced_val = ctx.coerce_to_type(*val, &init_base, &base_ty)?;
                        ctx.builder.def_var(*var, coerced_val);
                    }
                }
            }

            Ok(())
        }
        _ => Err(GlslError::new(
            ErrorCode::E0400,
            "only variable declarations supported",
        )),
    }
}

/// Parse type specifier from GLSL AST
pub fn parse_type_specifier<M: cranelift_module::Module>(
    _ctx: &CodegenContext<'_, M>,
    type_spec: &glsl::syntax::FullySpecifiedType,
) -> Result<crate::frontend::semantic::types::Type, GlslError> {
    // Use unified type parser from type_resolver.rs which handles arrays
    crate::frontend::semantic::type_resolver::parse_type_specifier(&type_spec.ty, None)
}

/// Emit initializer expression (returns values and type)
/// For arrays, returns a flat list of element values
pub fn emit_initializer<M: cranelift_module::Module>(
    ctx: &mut CodegenContext<'_, M>,
    init: &glsl::syntax::Initializer,
) -> Result<
    (
        Vec<cranelift_codegen::ir::Value>,
        crate::frontend::semantic::types::Type,
    ),
    GlslError,
> {
    use glsl::syntax::Initializer;

    match init {
        Initializer::Simple(expr) => ctx.emit_expr_typed(expr.as_ref()),
        Initializer::List(list) => {
            // Handle initializer list for arrays
            // For now, only support 1D arrays (flat list)
            let mut values = Vec::new();
            let mut element_type: Option<crate::frontend::semantic::types::Type> = None;

            for item in list.0.iter() {
                let (item_vals, item_ty) = emit_initializer(ctx, item)?;

                // For 1D arrays, each item should be a scalar (or we'll handle vectors later)
                if item_vals.len() != 1 {
                    return Err(GlslError::new(
                        ErrorCode::E0400,
                        format!(
                            "array initializer list items must be scalars (got {} values)",
                            item_vals.len()
                        ),
                    ));
                }

                // Track element type (should be consistent across all items)
                if let Some(ref expected_ty) = element_type {
                    // Type check: allow implicit conversions
                    crate::frontend::semantic::type_check::check_assignment(expected_ty, &item_ty)
                        .map_err(|_e| {
                            GlslError::new(
                                ErrorCode::E0400,
                                format!(
                                    "array initializer type mismatch: expected {expected_ty:?}, got {item_ty:?}"
                                ),
                            )
                        })?;
                } else {
                    element_type = Some(item_ty.clone());
                }

                values.push(item_vals[0]);
            }

            let element_ty = element_type
                .ok_or_else(|| GlslError::new(ErrorCode::E0400, "empty array initializer list"))?;

            // Return flat list of values and element type
            // The caller will handle creating the array type
            Ok((values, element_ty))
        }
    }
}
