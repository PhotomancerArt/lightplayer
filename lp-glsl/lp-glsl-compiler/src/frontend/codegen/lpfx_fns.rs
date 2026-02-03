//! Code generation for LPFX Functions
//!
//! Maps user-facing `lpfx_*` function names to internal implementations
//! and handles vector argument flattening.

use crate::DecimalFormat;
use crate::error::{ErrorCode, GlslError};
use crate::frontend::codegen::constants::{F32_ALIGN_SHIFT, F32_SIZE_BYTES};
use crate::frontend::codegen::context::CodegenContext;
use crate::frontend::semantic::lpfx::lpfx_fn_registry::find_lpfx_fn;
use crate::frontend::semantic::lpfx::lpfx_sig::build_call_signature;
use crate::semantic::types::Type;
use alloc::{format, vec, vec::Vec};
use cranelift_codegen::ir::{ExtFuncData, ExternalName, FuncRef, InstBuilder, MemFlags, Value};

impl<'a, M: cranelift_module::Module> CodegenContext<'a, M> {
    /// Emit code for an LPFX function call.
    ///
    /// # Arguments
    /// * `name` - Function name (e.g., "lpfx_hash1", "lpfx_snoise2")
    /// * `args` - Vector of (value, type) pairs for each argument
    ///
    /// # Returns
    /// Tuple of (result_values, return_type)
    pub fn emit_lp_lib_fn_call(
        &mut self,
        name: &str,
        args: Vec<(Vec<Value>, Type)>,
    ) -> Result<(Vec<Value>, Type), GlslError> {
        // Collect parameter types before flattening (needed for signature and overload resolution)
        let param_types: Vec<Type> = args.iter().map(|(_, ty)| ty.clone()).collect();

        // Look up function in registry with overload resolution
        let func = find_lpfx_fn(name, &param_types).ok_or_else(|| {
            GlslError::new(
                ErrorCode::E0400,
                format!(
                    "Unknown or ambiguous LPFX function: {name} with argument types {param_types:?}"
                ),
            )
        })?;

        // Prepare call arguments: handle out/inout as pointers, in as flattened components
        use crate::semantic::functions::ParamQualifier;
        let mut call_args = Vec::new();

        for ((vals, _arg_ty), param) in args.iter().zip(func.glsl_sig.parameters.iter()) {
            match param.qualifier {
                ParamQualifier::Out | ParamQualifier::InOut => {
                    // Out/inout: pass pointer directly (should be single pointer value)
                    if vals.len() != 1 {
                        return Err(GlslError::new(
                            ErrorCode::E0400,
                            format!(
                                "Expected single pointer value for out/inout parameter, got {} values",
                                vals.len()
                            ),
                        ));
                    }
                    call_args.push(vals[0]);
                }
                ParamQualifier::In => {
                    // In: flatten to components (existing behavior)
                    call_args.extend(vals);
                }
            }
        }

        // Check if function returns a vector (needs result pointer parameter)
        let return_type = &func.glsl_sig.return_type;
        let uses_result_ptr = return_type.is_vector();

        // Setup result buffer if needed (for vector returns)
        let return_buffer_ptr = if uses_result_ptr {
            let element_count = return_type.component_count().unwrap();
            let buffer_size = (element_count * F32_SIZE_BYTES) as u32;
            let pointer_type = self.gl_module.module_internal().isa().pointer_type();

            let slot = self.builder.func.create_sized_stack_slot(
                cranelift_codegen::ir::StackSlotData::new(
                    cranelift_codegen::ir::StackSlotKind::ExplicitSlot,
                    buffer_size,
                    F32_ALIGN_SHIFT,
                ),
            );

            Some(self.builder.ins().stack_addr(pointer_type, slot, 0))
        } else {
            None
        };

        // Prepare final call arguments: result pointer first (if present), then regular args
        // Note: result pointer is a normal parameter, not StructReturn
        let mut final_call_args = Vec::new();
        if let Some(buffer_ptr) = return_buffer_ptr {
            final_call_args.push(buffer_ptr);
        }
        final_call_args.extend(call_args);

        // Handle Decimal vs NonDecimal implementations
        match &func.impls {
            crate::frontend::semantic::lpfx::lpfx_fn::LpfxFnImpl::Decimal {
                float_impl, ..
            } => {
                // Always use float implementation in frontend - transform will convert to q32
                // Generate TestCase call with float signature (f32 args, f32 return)
                let func_ref = self.get_lpfx_testcase_call(func, *float_impl, &param_types)?;

                // Emit call instruction
                self.ensure_block()?;
                let call_inst = self.builder.ins().call(func_ref, &final_call_args);

                // Handle return values
                if let Some(buffer_ptr) = return_buffer_ptr {
                    // Vector return: load values from buffer (written by function via result pointer)
                    let element_count = return_type.component_count().unwrap();
                    let base_type = return_type.vector_base_type().unwrap();
                    let cranelift_ty = base_type.to_cranelift_type().map_err(|e| {
                        GlslError::new(
                            ErrorCode::E0400,
                            format!(
                                "Failed to convert return type to Cranelift type: {}",
                                e.message
                            ),
                        )
                    })?;

                    let mut loaded_vals = Vec::new();
                    for i in 0..element_count {
                        let offset = (i * F32_SIZE_BYTES) as i32;
                        let val = self.builder.ins().load(
                            cranelift_ty,
                            MemFlags::trusted(),
                            buffer_ptr,
                            offset,
                        );
                        loaded_vals.push(val);
                    }
                    Ok((loaded_vals, return_type.clone()))
                } else {
                    // Scalar return: extract from call results
                    let results = self.builder.inst_results(call_inst);
                    if results.len() != 1 {
                        return Err(GlslError::new(
                            ErrorCode::E0400,
                            format!(
                                "Expected 1 return value from LPFX function, got {}",
                                results.len()
                            ),
                        ));
                    }
                    Ok((vec![results[0]], return_type.clone()))
                }
            }
            crate::frontend::semantic::lpfx::lpfx_fn::LpfxFnImpl::NonDecimal(builtin_id) => {
                // Direct builtin call (hash functions don't need conversion)
                log::debug!(
                    "emit_lpfx_fn_call: Calling builtin {builtin_id:?} for LPFX function '{name}'"
                );
                let func_ref = self
                    .gl_module
                    .get_builtin_func_ref(*builtin_id, self.builder.func)?;

                // Emit call instruction
                self.ensure_block()?;
                let call_inst = self.builder.ins().call(func_ref, &final_call_args);

                // Handle return values
                if let Some(buffer_ptr) = return_buffer_ptr {
                    // Result pointer: load values from buffer
                    let element_count = return_type.component_count().unwrap();
                    let base_type = return_type.vector_base_type().unwrap();
                    let cranelift_ty = base_type.to_cranelift_type().map_err(|e| {
                        GlslError::new(
                            ErrorCode::E0400,
                            format!(
                                "Failed to convert return type to Cranelift type: {}",
                                e.message
                            ),
                        )
                    })?;

                    let mut loaded_vals = Vec::new();
                    for i in 0..element_count {
                        let offset = (i * F32_SIZE_BYTES) as i32;
                        let val = self.builder.ins().load(
                            cranelift_ty,
                            MemFlags::trusted(),
                            buffer_ptr,
                            offset,
                        );
                        loaded_vals.push(val);
                    }
                    Ok((loaded_vals, return_type.clone()))
                } else {
                    // Scalar return: extract from call results
                    let results = self.builder.inst_results(call_inst);
                    if results.len() != 1 {
                        return Err(GlslError::new(
                            ErrorCode::E0400,
                            format!(
                                "Expected 1 return value from LPFX function, got {}",
                                results.len()
                            ),
                        ));
                    }
                    Ok((vec![results[0]], return_type.clone()))
                }
            }
        }
    }

    /// Helper to declare and get FuncRef for LPFX function TestCase call.
    ///
    /// Creates external function calls using TestCase names based on builtin ID name
    /// (e.g., "__lpfx_hsv2rgb_f32" or "__lpfx_hsv2rgb_vec4_f32").
    /// These are converted to q32 builtins by the transform.
    ///
    /// Always uses float signature (f32 args, f32 return) - the transform will handle
    /// conversion to q32 when processing the TestCase call.
    fn get_lpfx_testcase_call(
        &mut self,
        func: &'static crate::frontend::semantic::lpfx::lpfx_fn::LpfxFn,
        builtin_id: crate::backend::builtins::registry::BuiltinId,
        _param_types: &[Type],
    ) -> Result<FuncRef, GlslError> {
        // TestCase name is the builtin ID name (includes variant info for overloads)
        let testcase_name = builtin_id.name();

        // Get pointer type for result pointer parameter (if needed for vector returns)
        let pointer_type = self.gl_module.module_internal().isa().pointer_type();

        // Debug: Log pointer type to help diagnose architecture-specific issues
        log::debug!(
            "get_lpfx_testcase_call: function={testcase_name}, pointer_type={pointer_type:?}"
        );

        // Build signature with Float format (f32 args, f32 return)
        // The transform will convert this to q32 when processing the call
        let sig = build_call_signature(func, builtin_id, DecimalFormat::Float, pointer_type);
        if func.glsl_sig.return_type.is_vector() {
            let _result_ptr_param = sig
                .params
                .first()
                .expect("Result pointer param should exist");
            log::trace!(
                "get_lpfx_testcase_call: Result pointer param type={:?}, purpose={:?} (should be Normal, not StructReturn)",
                _result_ptr_param.value_type,
                _result_ptr_param.purpose
            );
        }

        // Verify signature before importing (to catch issues early)
        let built_result_ptr_type = if func.glsl_sig.return_type.is_vector() {
            Some(
                sig.params
                    .first()
                    .expect("Result pointer param should exist")
                    .value_type,
            )
        } else {
            None
        };
        if let Some(built_type) = built_result_ptr_type {
            if built_type != pointer_type {
                return Err(GlslError::new(
                    ErrorCode::E0400,
                    format!(
                        "Signature build error: Built signature has result pointer type {built_type:?}, expected {pointer_type:?} (ISA pointer type)."
                    ),
                ));
            }
        }

        // Create TestCase name for external function call
        let sig_ref = self.builder.func.import_signature(sig.clone());

        // Verify the imported signature matches what we built (check for deduplication issues)
        let imported_sig = &self.builder.func.dfg.signatures[sig_ref];
        if func.glsl_sig.return_type.is_vector() {
            let result_ptr_param = imported_sig
                .params
                .first()
                .expect("Result pointer param should exist");
            if result_ptr_param.value_type != pointer_type {
                return Err(GlslError::new(
                    ErrorCode::E0400,
                    format!(
                        "Signature mismatch after import: Result pointer param type is {:?}, expected {:?} (ISA pointer type). \
                         Built signature had {:?}. This indicates a signature deduplication bug - Cranelift matched to an old signature with wrong pointer type.",
                        result_ptr_param.value_type, pointer_type, built_result_ptr_type
                    ),
                ));
            }
        }

        let ext_name = ExternalName::testcase(testcase_name.as_bytes());
        let ext_func = ExtFuncData {
            name: ext_name,
            signature: sig_ref,
            colocated: false,
        };
        Ok(self.builder.func.import_function(ext_func))
    }
}
