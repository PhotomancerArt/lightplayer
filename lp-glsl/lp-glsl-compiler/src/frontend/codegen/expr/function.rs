use crate::error::{ErrorCode, GlslError, source_span_to_location};
use crate::frontend::codegen::context::CodegenContext;
use crate::frontend::codegen::rvalue::RValue;
use crate::semantic::type_check::{is_matrix_type_name, is_scalar_type_name, is_vector_type_name};
use crate::semantic::types::Type as GlslType;
use cranelift_codegen::ir::InstBuilder;
use glsl::syntax::Expr;

use super::coercion;
use super::constructor;

use alloc::{format, vec, vec::Vec};

/// Emit code to compute a function call as an RValue
pub fn emit_function_call_rvalue<M: cranelift_module::Module>(
    ctx: &mut CodegenContext<'_, M>,
    expr: &Expr,
) -> Result<RValue, GlslError> {
    // Ensure we're in a block before evaluating
    ctx.ensure_block()?;

    let (vals, ty) = emit_function_call(ctx, expr)?;
    Ok(RValue::from_aggregate(vals, ty))
}

/// Legacy function for backwards compatibility
pub fn emit_function_call<M: cranelift_module::Module>(
    ctx: &mut CodegenContext<'_, M>,
    expr: &Expr,
) -> Result<(Vec<cranelift_codegen::ir::Value>, GlslType), GlslError> {
    let Expr::FunCall(func_ident, args, span) = expr else {
        unreachable!("translate_function_call called on non-call");
    };

    let func_name = match func_ident {
        glsl::syntax::FunIdentifier::Identifier(ident) => &ident.name,
        _ => {
            return Err(GlslError::new(
                ErrorCode::E0400,
                "complex function identifiers not yet supported",
            ));
        }
    };

    // Check if it's a type constructor
    if is_vector_type_name(func_name) {
        return constructor::emit_vector_constructor(ctx, func_name, args, span.clone());
    }

    if is_matrix_type_name(func_name) {
        return constructor::emit_matrix_constructor(ctx, func_name, args);
    }

    // Check for scalar constructors
    if is_scalar_type_name(func_name) {
        return constructor::emit_scalar_constructor(ctx, func_name, args, span.clone());
    }

    // Check if it's a built-in function
    if crate::frontend::semantic::builtins::is_builtin_function(func_name) {
        return emit_builtin_call_expr(ctx, func_name, args, span.clone());
    }

    // Check if it's an LPFX function
    if crate::frontend::semantic::lpfx::lpfx_fn_registry::is_lpfx_fn(func_name) {
        return emit_lp_lib_fn_call_expr(ctx, func_name, args, span.clone());
    }

    // User-defined function
    emit_user_function_call(ctx, func_name, args, span.clone())
}

fn emit_builtin_call_expr<M: cranelift_module::Module>(
    ctx: &mut CodegenContext<'_, M>,
    name: &str,
    args: &[glsl::syntax::Expr],
    call_span: glsl::syntax::SourceSpan,
) -> Result<(Vec<cranelift_codegen::ir::Value>, GlslType), GlslError> {
    // Translate all arguments
    let mut translated_args = Vec::new();
    let mut arg_types = Vec::new();

    for arg in args {
        let (vals, ty) = ctx.emit_expr_typed(arg)?;
        translated_args.push((vals, ty.clone()));
        arg_types.push(ty);
    }

    // Validate builtin call before codegen
    match crate::frontend::semantic::builtins::check_builtin_call(name, &arg_types) {
        Ok(_return_type) => {
            // Validation passed, proceed with codegen
        }
        Err(err_msg) => {
            // Convert validation error to GlslError
            let error = GlslError::new(crate::error::ErrorCode::E0114, err_msg)
                .with_location(source_span_to_location(&call_span));
            return Err(ctx.add_span_to_error(error, &call_span));
        }
    }

    // Delegate to built-in implementation and add span to any errors
    log::debug!("emit_builtin_call_expr: Calling builtin function '{name}'");
    match ctx.emit_builtin_call(name, translated_args) {
        Ok(result) => Ok(result),
        Err(mut error) => {
            // Add location and span_text if not already present
            if error.location.is_none() {
                error = error.with_location(source_span_to_location(&call_span));
            }
            Err(ctx.add_span_to_error(error, &call_span))
        }
    }
}

fn emit_lp_lib_fn_call_expr<M: cranelift_module::Module>(
    ctx: &mut CodegenContext<'_, M>,
    name: &str,
    args: &[glsl::syntax::Expr],
    call_span: glsl::syntax::SourceSpan,
) -> Result<(Vec<cranelift_codegen::ir::Value>, GlslType), GlslError> {
    use crate::frontend::codegen::constants::F32_SIZE_BYTES;
    use crate::frontend::codegen::lvalue::{read_lvalue, resolve_lvalue};
    use crate::semantic::functions::ParamQualifier;

    // Get function signature to check for out/inout parameters
    let (_, arg_types) = prepare_function_arguments(ctx, args)?;
    let func = crate::frontend::semantic::lpfx::lpfx_fn_registry::find_lpfx_fn(name, &arg_types)
        .ok_or_else(|| {
            GlslError::new(
                crate::error::ErrorCode::E0400,
                format!("Unknown LPFX function: {name}"),
            )
            .with_location(source_span_to_location(&call_span))
        })?;

    // Validate LPFX function call before codegen
    match crate::frontend::semantic::lpfx::lpfx_fn_registry::check_lpfx_fn_call(name, &arg_types) {
        Ok(_return_type) => {
            // Validation passed, proceed with codegen
        }
        Err(err_msg) => {
            // Convert validation error to GlslError
            let error = GlslError::new(crate::error::ErrorCode::E0114, err_msg)
                .with_location(source_span_to_location(&call_span));
            return Err(ctx.add_span_to_error(error, &call_span));
        }
    }

    // Prepare arguments: handle out/inout by getting addresses, in by evaluating expressions
    let mut translated_args = Vec::new();
    let mut out_inout_args = Vec::new();
    let pointer_type = ctx.gl_module.module_internal().isa().pointer_type();

    for (arg_expr, param) in args.iter().zip(func.glsl_sig.parameters.iter()) {
        match param.qualifier {
            ParamQualifier::Out | ParamQualifier::InOut => {
                // Out/inout: resolve as lvalue and get address
                let lvalue = resolve_lvalue(ctx, arg_expr).map_err(|mut err| {
                    if err.message.contains("not a valid LValue") {
                        err.message = format!(
                            "out/inout parameter requires an lvalue (variable, array element, etc.), but got an expression"
                        );
                    }
                    err
                })?;

                // For arrays: use existing array pointer directly (no copy-back needed)
                let stack_slot_ptr = if param.ty.is_array() {
                    match &lvalue {
                        crate::frontend::codegen::lvalue::LValue::PointerBased { ptr, .. } => {
                            // Out/inout arrays use PointerBased variant
                            *ptr
                        }
                        crate::frontend::codegen::lvalue::LValue::Variable { .. } => {
                            // Regular arrays: extract variable name from expression to look up array_ptr
                            if let glsl::syntax::Expr::Variable(ident, _) = arg_expr {
                                if let Some(var_info) = ctx.lookup_var_info(&ident.name) {
                                    if let Some(arr_ptr) = var_info.array_ptr {
                                        // Use existing array pointer - function writes directly to array
                                        arr_ptr
                                    } else {
                                        return Err(GlslError::new(
                                            ErrorCode::E0400,
                                            format!(
                                                "Array variable {} does not have array pointer",
                                                ident.name
                                            ),
                                        ));
                                    }
                                } else {
                                    return Err(GlslError::new(
                                        ErrorCode::E0400,
                                        format!("Variable {} not found", ident.name),
                                    ));
                                }
                            } else {
                                return Err(GlslError::new(
                                    ErrorCode::E0400,
                                    "Array out/inout parameter must be array variable, not array element",
                                ));
                            }
                        }
                        _ => {
                            return Err(GlslError::new(
                                ErrorCode::E0400,
                                "Array out/inout parameter must be array variable, not array element",
                            ));
                        }
                    }
                } else {
                    // Non-array: create temporary stack slot
                    let size_bytes = if param.ty.is_vector() {
                        let component_count = param.ty.component_count().unwrap();
                        component_count * F32_SIZE_BYTES
                    } else if param.ty.is_matrix() {
                        let element_count = param.ty.matrix_element_count().unwrap();
                        element_count * F32_SIZE_BYTES
                    } else {
                        F32_SIZE_BYTES
                    };

                    // Allocate stack slot for out/inout parameter
                    let stack_slot = ctx.builder.func.create_sized_stack_slot(
                        cranelift_codegen::ir::StackSlotData::new(
                            cranelift_codegen::ir::StackSlotKind::ExplicitSlot,
                            size_bytes as u32,
                            crate::frontend::codegen::constants::F32_ALIGN_SHIFT,
                        ),
                    );
                    let ptr = ctx.builder.ins().stack_addr(pointer_type, stack_slot, 0);

                    // For inout: copy current value to stack slot before call
                    if param.qualifier == ParamQualifier::InOut {
                        let (current_vals, _) = read_lvalue(ctx, &lvalue)?;
                        let flags = cranelift_codegen::ir::MemFlags::trusted();
                        for (i, &val) in current_vals.iter().enumerate() {
                            let offset = (i * F32_SIZE_BYTES) as i32;
                            ctx.builder.ins().store(flags, val, ptr, offset);
                        }
                    }

                    ptr
                };

                // Pass pointer as argument
                translated_args.push((vec![stack_slot_ptr], param.ty.clone()));

                // Store info for copy-back after call (only for non-arrays, arrays are written directly)
                if !param.ty.is_array() {
                    out_inout_args.push(OutInoutArgInfo {
                        lvalue,
                        param_ty: param.ty.clone(),
                        stack_slot_ptr,
                    });
                }
            }
            ParamQualifier::In => {
                // In: evaluate expression normally
                let (vals, ty) = ctx.emit_expr_typed(arg_expr)?;
                translated_args.push((vals, ty));
            }
        }
    }

    // Delegate to LP library function implementation
    let result = ctx.emit_lp_lib_fn_call(name, translated_args);

    // Copy back out/inout parameters
    copy_back_out_parameters(ctx, &out_inout_args)?;

    // Return result with span handling
    match result {
        Ok((result_vals, return_type)) => Ok((result_vals, return_type)),
        Err(mut error) => {
            // Add location and span_text if not already present
            if error.location.is_none() {
                error = error.with_location(source_span_to_location(&call_span));
            }
            Err(ctx.add_span_to_error(error, &call_span))
        }
    }
}

/// Prepare function call arguments by translating expressions
fn prepare_function_arguments<M: cranelift_module::Module>(
    ctx: &mut CodegenContext<'_, M>,
    args: &[glsl::syntax::Expr],
) -> Result<(Vec<cranelift_codegen::ir::Value>, Vec<GlslType>), GlslError> {
    let mut arg_vals_flat = Vec::new();
    let mut arg_types = Vec::new();

    for arg in args {
        let (vals, ty) = ctx.emit_expr_typed(arg)?;
        arg_vals_flat.extend(vals);
        arg_types.push(ty);
    }

    Ok((arg_vals_flat, arg_types))
}

/// Lookup function ID and signature from registry
fn lookup_function_signature<M: cranelift_module::Module>(
    ctx: &CodegenContext<'_, M>,
    name: &str,
    arg_types: &[GlslType],
    call_span: &glsl::syntax::SourceSpan,
) -> Result<
    (
        cranelift_module::FuncId,
        crate::frontend::semantic::functions::FunctionSignature,
    ),
    GlslError,
> {
    let func_ids = ctx
        .function_ids
        .as_ref()
        .ok_or_else(|| GlslError::new(ErrorCode::E0400, "function IDs not set (internal error)"))?;
    let func_registry = ctx.function_registry.ok_or_else(|| {
        GlslError::new(
            ErrorCode::E0400,
            "function registry not set (internal error)",
        )
    })?;

    let func_id = func_ids.get(name).ok_or_else(|| {
        let error = GlslError::undefined_function(name);
        if error.location.is_none() {
            error.with_location(crate::error::source_span_to_location(call_span))
        } else {
            error
        }
    })?;

    let func_sig = match func_registry.lookup_function(name, arg_types) {
        Ok(sig) => sig.clone(),
        Err(mut error) => {
            if error.location.is_none() {
                error = error.with_location(crate::error::source_span_to_location(call_span));
            }
            return Err(ctx.add_span_to_error(error, call_span));
        }
    };

    Ok((*func_id, func_sig))
}

/// Validate that function call arguments can be coerced to parameter types
/// Also validates that out/inout arguments are lvalues
fn validate_function_call<M: cranelift_module::Module>(
    ctx: &mut CodegenContext<'_, M>,
    func_sig: &crate::frontend::semantic::functions::FunctionSignature,
    args: &[glsl::syntax::Expr],
    arg_types: &[GlslType],
    name: &str,
    call_span: &glsl::syntax::SourceSpan,
) -> Result<(), GlslError> {
    use crate::frontend::codegen::lvalue::resolve_lvalue;
    use crate::semantic::functions::ParamQualifier;

    // Validate out/inout arguments are lvalues
    for (param, arg_expr) in func_sig.parameters.iter().zip(args) {
        match param.qualifier {
            ParamQualifier::Out | ParamQualifier::InOut => {
                // Try to resolve as lvalue - this will fail with a good error if not an lvalue
                resolve_lvalue(ctx, arg_expr).map_err(|mut err| {
                    // Improve error message for out/inout parameters
                    if err.message.contains("not a valid LValue") {
                        err.message = format!(
                            "out/inout parameter requires an lvalue (variable, array element, etc.), but got an expression"
                        );
                    }
                    err
                })?;
            }
            ParamQualifier::In => {
                // In parameters don't need lvalue validation
            }
        }
    }

    // Validate type compatibility
    for (param, arg_ty) in func_sig.parameters.iter().zip(arg_types) {
        let arg_base = if arg_ty.is_vector() {
            arg_ty.vector_base_type().unwrap()
        } else {
            arg_ty.clone()
        };
        let param_base = if param.ty.is_vector() {
            param.ty.vector_base_type().unwrap()
        } else {
            param.ty.clone()
        };

        if arg_base != param_base
            && !crate::frontend::semantic::type_check::can_implicitly_convert(
                &arg_base,
                &param_base,
            )
        {
            let expected_count: usize = func_sig
                .parameters
                .iter()
                .map(|p| {
                    if p.ty.is_vector() {
                        p.ty.component_count().unwrap()
                    } else {
                        1
                    }
                })
                .sum();
            let error = GlslError::new(
                ErrorCode::E0400,
                format!(
                    "function parameter mismatch: expected {expected_count} block parameters, got 0"
                ),
            )
            .with_location(crate::error::source_span_to_location(call_span))
            .with_note(format!(
                "function `{}` expects parameter of type `{:?}`, got `{:?}`",
                name, param.ty, arg_ty
            ));
            return Err(ctx.add_span_to_error(error, call_span));
        }
    }
    Ok(())
}

/// Setup StructReturn buffer if the function uses it
fn setup_struct_return_buffer<M: cranelift_module::Module>(
    ctx: &mut CodegenContext<'_, M>,
    func_sig: &crate::frontend::semantic::functions::FunctionSignature,
    func_ref: cranelift_codegen::ir::FuncRef,
) -> Result<Option<cranelift_codegen::ir::Value>, GlslError> {
    let ext_func_data = &ctx.builder.func.dfg.ext_funcs[func_ref];
    let callee_sig_ref = ext_func_data.signature;
    let callee_sig = &ctx.builder.func.dfg.signatures[callee_sig_ref];

    let uses_sret = callee_sig
        .params
        .iter()
        .any(|p| p.purpose == cranelift_codegen::ir::ArgumentPurpose::StructReturn);

    if !uses_sret {
        return Ok(None);
    }

    let element_count = if func_sig.return_type.is_vector() {
        func_sig.return_type.component_count().unwrap()
    } else if func_sig.return_type.is_matrix() {
        func_sig.return_type.matrix_element_count().unwrap()
    } else {
        return Err(GlslError::new(
            ErrorCode::E0400,
            "StructReturn used but return type is not composite",
        ));
    };

    let buffer_size = (element_count * crate::frontend::codegen::constants::F32_SIZE_BYTES) as u32;
    let pointer_type = ctx.gl_module.module_internal().isa().pointer_type();

    let slot = ctx
        .builder
        .func
        .create_sized_stack_slot(cranelift_codegen::ir::StackSlotData::new(
            cranelift_codegen::ir::StackSlotKind::ExplicitSlot,
            buffer_size,
            crate::frontend::codegen::constants::F32_ALIGN_SHIFT,
        ));

    Ok(Some(ctx.builder.ins().stack_addr(pointer_type, slot, 0)))
}

/// Information about out/inout arguments for copy-back after call
struct OutInoutArgInfo {
    lvalue: crate::frontend::codegen::lvalue::LValue,
    param_ty: GlslType,
    stack_slot_ptr: cranelift_codegen::ir::Value,
}

/// Prepare call arguments with coercion
/// Returns call arguments and information about out/inout parameters for copy-back
fn prepare_call_arguments<M: cranelift_module::Module>(
    ctx: &mut CodegenContext<'_, M>,
    func_sig: &crate::frontend::semantic::functions::FunctionSignature,
    args: &[glsl::syntax::Expr],
    arg_types: &[GlslType],
    return_buffer_ptr: Option<cranelift_codegen::ir::Value>,
    call_span: &glsl::syntax::SourceSpan,
) -> Result<(Vec<cranelift_codegen::ir::Value>, Vec<OutInoutArgInfo>), GlslError> {
    use crate::frontend::codegen::constants::F32_SIZE_BYTES;
    use crate::frontend::codegen::lvalue::{read_lvalue, resolve_lvalue};
    use crate::semantic::functions::ParamQualifier;

    let mut call_args = Vec::new();
    let mut out_inout_args = Vec::new();

    // Add StructReturn parameter first if present
    if let Some(buffer_ptr) = return_buffer_ptr {
        call_args.push(buffer_ptr);
    }

    // Add all parameters
    for (glsl_param_idx, param) in func_sig.parameters.iter().enumerate() {
        let arg_expr = &args[glsl_param_idx];
        let arg_ty = &arg_types[glsl_param_idx];
        let pointer_type = ctx.gl_module.module_internal().isa().pointer_type();

        match param.qualifier {
            ParamQualifier::Out | ParamQualifier::InOut => {
                // Out/inout parameters: validate argument is an lvalue, then resolve and get address
                // Try to resolve as lvalue - this will fail with a good error if not an lvalue
                let lvalue = resolve_lvalue(ctx, arg_expr).map_err(|mut err| {
                    // Improve error message for out/inout parameters
                    if err.message.contains("not a valid LValue") {
                        err.message = format!(
                            "out/inout parameter requires an lvalue (variable, array element, etc.), but got an expression"
                        );
                    }
                    err
                })?;

                // For arrays: use existing array pointer directly (no copy-back needed)
                let stack_slot_ptr = if param.ty.is_array() {
                    match &lvalue {
                        crate::frontend::codegen::lvalue::LValue::PointerBased { ptr, .. } => {
                            // Out/inout arrays use PointerBased variant
                            *ptr
                        }
                        crate::frontend::codegen::lvalue::LValue::Variable { .. } => {
                            // Regular arrays: extract variable name from expression to look up array_ptr
                            if let glsl::syntax::Expr::Variable(ident, _) = arg_expr {
                                if let Some(var_info) = ctx.lookup_var_info(&ident.name) {
                                    if let Some(arr_ptr) = var_info.array_ptr {
                                        // Use existing array pointer - function writes directly to array
                                        arr_ptr
                                    } else {
                                        return Err(GlslError::new(
                                            ErrorCode::E0400,
                                            format!(
                                                "Array variable {} does not have array pointer",
                                                ident.name
                                            ),
                                        ));
                                    }
                                } else {
                                    return Err(GlslError::new(
                                        ErrorCode::E0400,
                                        format!("Variable {} not found", ident.name),
                                    ));
                                }
                            } else {
                                return Err(GlslError::new(
                                    ErrorCode::E0400,
                                    "Array out/inout parameter must be array variable, not array element",
                                ));
                            }
                        }
                        _ => {
                            return Err(GlslError::new(
                                ErrorCode::E0400,
                                "Array out/inout parameter must be array variable, not array element",
                            ));
                        }
                    }
                } else {
                    // Non-array: create temporary stack slot
                    let size_bytes = if param.ty.is_vector() {
                        let component_count = param.ty.component_count().unwrap();
                        component_count * F32_SIZE_BYTES
                    } else if param.ty.is_matrix() {
                        let element_count = param.ty.matrix_element_count().unwrap();
                        element_count * F32_SIZE_BYTES
                    } else {
                        F32_SIZE_BYTES
                    };

                    // Allocate stack slot for out/inout parameter
                    let stack_slot = ctx.builder.func.create_sized_stack_slot(
                        cranelift_codegen::ir::StackSlotData::new(
                            cranelift_codegen::ir::StackSlotKind::ExplicitSlot,
                            size_bytes as u32,
                            crate::frontend::codegen::constants::F32_ALIGN_SHIFT,
                        ),
                    );
                    let ptr = ctx.builder.ins().stack_addr(pointer_type, stack_slot, 0);

                    // For inout: copy current value to stack slot before call
                    if param.qualifier == ParamQualifier::InOut {
                        let (current_vals, _) = read_lvalue(ctx, &lvalue)?;

                        let flags = cranelift_codegen::ir::MemFlags::trusted();
                        for (i, &val) in current_vals.iter().enumerate() {
                            let offset = (i * F32_SIZE_BYTES) as i32;
                            ctx.builder.ins().store(flags, val, ptr, offset);
                        }
                    }

                    ptr
                };

                // Pass pointer as argument
                call_args.push(stack_slot_ptr);

                // Store info for copy-back after call (only for non-arrays, arrays are written directly)
                if !param.ty.is_array() {
                    out_inout_args.push(OutInoutArgInfo {
                        lvalue,
                        param_ty: param.ty.clone(),
                        stack_slot_ptr,
                    });
                }
            }
            ParamQualifier::In => {
                // In parameters: evaluate expression and expand to components (existing behavior)
                let (arg_vals_flat, _) = ctx.emit_expr_typed(arg_expr)?;
                let mut arg_val_idx = 0;

                let component_count = if param.ty.is_vector() {
                    param.ty.component_count().unwrap()
                } else if param.ty.is_matrix() {
                    param.ty.matrix_element_count().unwrap()
                } else {
                    1
                };

                let arg_base = if arg_ty.is_vector() {
                    arg_ty.vector_base_type().unwrap()
                } else {
                    arg_ty.clone()
                };
                let param_base = if param.ty.is_vector() {
                    param.ty.vector_base_type().unwrap()
                } else {
                    param.ty.clone()
                };

                for _ in 0..component_count {
                    if arg_val_idx >= arg_vals_flat.len() {
                        return Err(GlslError::new(
                            ErrorCode::E0400,
                            format!("Not enough argument values for parameter {glsl_param_idx}"),
                        ));
                    }

                    let arg_val = arg_vals_flat[arg_val_idx];
                    let converted = coercion::coerce_to_type_with_location(
                        ctx,
                        arg_val,
                        &arg_base,
                        &param_base,
                        Some(call_span.clone()),
                    )?;
                    call_args.push(converted);
                    arg_val_idx += 1;
                }
            }
        }
    }

    Ok((call_args, out_inout_args))
}

/// Copy back values from out/inout parameters to original lvalues
fn copy_back_out_parameters<M: cranelift_module::Module>(
    ctx: &mut CodegenContext<'_, M>,
    out_inout_args: &[OutInoutArgInfo],
) -> Result<(), GlslError> {
    use crate::frontend::codegen::constants::F32_SIZE_BYTES;
    use crate::frontend::codegen::lvalue::write_lvalue;

    let flags = cranelift_codegen::ir::MemFlags::trusted();

    for arg_info in out_inout_args {
        // Load values from stack slot
        let component_count = if arg_info.param_ty.is_vector() {
            arg_info.param_ty.component_count().unwrap()
        } else if arg_info.param_ty.is_matrix() {
            arg_info.param_ty.matrix_element_count().unwrap()
        } else {
            1
        };

        let base_cranelift_ty = if arg_info.param_ty.is_vector() {
            arg_info
                .param_ty
                .vector_base_type()
                .unwrap()
                .to_cranelift_type()
                .map_err(|e| {
                    GlslError::new(
                        ErrorCode::E0400,
                        format!("Failed to convert type: {}", e.message),
                    )
                })?
        } else if arg_info.param_ty.is_matrix() {
            cranelift_codegen::ir::types::F32
        } else {
            arg_info.param_ty.to_cranelift_type().map_err(|e| {
                GlslError::new(
                    ErrorCode::E0400,
                    format!("Failed to convert type: {}", e.message),
                )
            })?
        };

        let mut loaded_vals = Vec::new();
        for i in 0..component_count {
            let offset = (i * F32_SIZE_BYTES) as i32;
            let val =
                ctx.builder
                    .ins()
                    .load(base_cranelift_ty, flags, arg_info.stack_slot_ptr, offset);
            loaded_vals.push(val);
        }

        // Write back to original lvalue
        write_lvalue(ctx, &arg_info.lvalue, &loaded_vals)?;
    }

    Ok(())
}

/// Execute function call and get return values
fn execute_function_call<M: cranelift_module::Module>(
    ctx: &mut CodegenContext<'_, M>,
    func_ref: cranelift_codegen::ir::FuncRef,
    call_args: &[cranelift_codegen::ir::Value],
    func_sig: &crate::frontend::semantic::functions::FunctionSignature,
    return_buffer_ptr: Option<cranelift_codegen::ir::Value>,
) -> Result<Vec<cranelift_codegen::ir::Value>, GlslError> {
    // Ensure we're in a block before making the call
    ctx.ensure_block()?;
    let call_inst = ctx.builder.ins().call(func_ref, call_args);

    if let Some(buffer_ptr) = return_buffer_ptr {
        let element_count = if func_sig.return_type.is_vector() {
            func_sig.return_type.component_count().unwrap()
        } else if func_sig.return_type.is_matrix() {
            func_sig.return_type.matrix_element_count().unwrap()
        } else {
            return Err(GlslError::new(
                ErrorCode::E0400,
                "StructReturn used but return type is not composite",
            ));
        };

        // Determine the base type and corresponding Cranelift IR type
        let base_type = if func_sig.return_type.is_vector() {
            func_sig.return_type.vector_base_type().unwrap()
        } else {
            // Matrices are always float
            crate::frontend::semantic::types::Type::Float
        };

        let cranelift_ty = base_type.to_cranelift_type().map_err(|e| {
            GlslError::new(
                ErrorCode::E0400,
                format!(
                    "Failed to convert return type to Cranelift type: {}",
                    e.message
                ),
            )
        })?;

        log::trace!(
            "execute_function_call: loading {element_count} elements of type {base_type:?} (cranelift_ty={cranelift_ty:?})"
        );
        let mut loaded_vals = Vec::new();
        for i in 0..element_count {
            let offset = (i * crate::frontend::codegen::constants::F32_SIZE_BYTES) as i32;
            log::trace!("  loading element {i} at offset {offset}, cranelift_ty={cranelift_ty:?}");
            let val = ctx.builder.ins().load(
                cranelift_ty,
                cranelift_codegen::ir::MemFlags::trusted(),
                buffer_ptr,
                offset,
            );
            log::trace!("    loaded val = {val:?} (should be {cranelift_ty:?})");
            loaded_vals.push(val);
        }
        log::trace!(
            "  execute_function_call: returning {} loaded values",
            loaded_vals.len()
        );
        Ok(loaded_vals)
    } else {
        Ok(ctx.builder.inst_results(call_inst).to_vec())
    }
}

/// Package return values according to return type
fn package_return_values(
    return_vals: Vec<cranelift_codegen::ir::Value>,
    return_type: &GlslType,
) -> Result<(Vec<cranelift_codegen::ir::Value>, GlslType), GlslError> {
    if *return_type == GlslType::Void {
        Ok((vec![], GlslType::Void))
    } else if return_type.is_vector() {
        let count = return_type.component_count().unwrap();
        Ok((return_vals[0..count].to_vec(), return_type.clone()))
    } else if return_type.is_matrix() {
        let count = return_type.matrix_element_count().unwrap();
        Ok((return_vals[0..count].to_vec(), return_type.clone()))
    } else {
        Ok((vec![return_vals[0]], return_type.clone()))
    }
}

fn emit_user_function_call<M: cranelift_module::Module>(
    ctx: &mut CodegenContext<'_, M>,
    name: &str,
    args: &[glsl::syntax::Expr],
    call_span: glsl::syntax::SourceSpan,
) -> Result<(Vec<cranelift_codegen::ir::Value>, GlslType), GlslError> {
    // Step 1: Prepare arguments (get types for lookup)
    let (_, arg_types) = prepare_function_arguments(ctx, args)?;

    // Step 2: Lookup function signature
    let (func_id, func_sig) = lookup_function_signature(ctx, name, &arg_types, &call_span)?;

    // Step 3: Validate call (including lvalue validation for out/inout)
    validate_function_call(ctx, &func_sig, args, &arg_types, name, &call_span)?;

    // Step 4: Import function and setup StructReturn if needed
    let func_ref = ctx
        .gl_module
        .module_mut_internal()
        .declare_func_in_func(func_id, ctx.builder.func);
    let return_buffer_ptr = setup_struct_return_buffer(ctx, &func_sig, func_ref)?;

    // Step 5: Prepare call arguments (now handles out/inout)
    let (call_args, out_inout_args) = prepare_call_arguments(
        ctx,
        &func_sig,
        args,
        &arg_types,
        return_buffer_ptr,
        &call_span,
    )?;

    // Step 6: Execute call
    let return_vals =
        execute_function_call(ctx, func_ref, &call_args, &func_sig, return_buffer_ptr)?;

    // Step 7: Copy back out/inout parameters
    copy_back_out_parameters(ctx, &out_inout_args)?;
    log::trace!(
        "translate_user_function_call: loaded {} return values, func_sig.return_type={:?}",
        return_vals.len(),
        func_sig.return_type
    );
    for (_i, _val) in return_vals.iter().enumerate() {
        log::trace!("  return_vals[{_i}] = {_val:?}");
    }

    // Step 8: Package return values
    let (packaged_vals, packaged_ty) = package_return_values(return_vals, &func_sig.return_type)?;
    log::trace!(
        "translate_user_function_call: packaged to {} values, type={:?}",
        packaged_vals.len(),
        packaged_ty
    );
    for (_i, _val) in packaged_vals.iter().enumerate() {
        log::trace!("  packaged_vals[{_i}] = {_val:?}");
    }
    Ok((packaged_vals, packaged_ty))
}
