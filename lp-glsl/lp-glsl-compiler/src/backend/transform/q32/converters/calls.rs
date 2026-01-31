//! Function call conversion functions.

use crate::backend::builtins::registry::BuiltinId;
use crate::backend::transform::q32::converters::math::map_testcase_to_builtin;
use crate::backend::transform::q32::converters::{get_first_result, map_value};
use crate::backend::transform::q32::signature::convert_signature;
use crate::backend::transform::q32::types::FixedPointFormat;
use crate::error::{ErrorCode, GlslError};
use crate::frontend::semantic::lpfx::lpfx_fn_registry::find_lpfx_fn_by_builtin_id;
use alloc::{format, string::String, vec::Vec};
use cranelift_codegen::ir::{
    ExtFuncData, ExternalName, FuncRef, Function, Inst, InstBuilder, InstructionData, SigRef,
    UserExternalName, Value, types,
};
use cranelift_frontend::FunctionBuilder;
use cranelift_module::FuncId;
use hashbrown::HashMap;

/// Maps an external function reference to a new function reference with converted signature.
///
/// Preserves function identity by name, not just signature. Functions with identical
/// signatures after transformation (e.g., mul_mat2 and mul_vec4) must map to distinct FuncRefs.
pub(crate) fn map_external_function(
    old_func: &Function,
    old_func_ref: FuncRef,
    builder: &mut FunctionBuilder,
    ext_func_map: &mut HashMap<FuncRef, FuncRef>,
    sig_map: &mut HashMap<SigRef, SigRef>,
    format: FixedPointFormat,
    func_id_map: &HashMap<String, FuncId>,
    old_func_id_map: &HashMap<FuncId, String>,
) -> Result<FuncRef, GlslError> {
    if let Some(&new_func_ref) = ext_func_map.get(&old_func_ref) {
        return Ok(new_func_ref);
    }

    let old_ext_func = &old_func.dfg.ext_funcs[old_func_ref];
    let old_sig_ref = old_ext_func.signature;

    let new_sig_ref = if let Some(&mapped_sig_ref) = sig_map.get(&old_sig_ref) {
        mapped_sig_ref
    } else {
        let old_sig = &old_func.dfg.signatures[old_sig_ref];
        let new_sig = convert_signature(old_sig, format);
        let new_sig_ref = builder.func.import_signature(new_sig);
        sig_map.insert(old_sig_ref, new_sig_ref);
        new_sig_ref
    };

    // Extract actual UserExternalName (namespace:index) from old function and create
    // new reference in new function. This ensures mapping by function identity rather
    // than reference index, which is critical when multiple functions share signatures.
    //
    // IMPORTANT: The UserExternalName.index contains the OLD FuncId. We need to map it
    // to the NEW FuncId by: old FuncId -> function name -> new FuncId
    let new_name = match &old_ext_func.name {
        ExternalName::User(old_user_ref) => {
            let user_name = old_func
                .params
                .user_named_funcs()
                .get(*old_user_ref)
                .cloned()
                .ok_or_else(|| {
                    GlslError::new(
                        ErrorCode::E0400,
                        alloc::format!(
                            "UserExternalNameRef {old_user_ref} not found in old function's user_named_funcs"
                        ),
                    )
                })?;

            // Map old FuncId to new FuncId via function name
            let old_func_id = FuncId::from_u32(user_name.index);
            let func_name = old_func_id_map.get(&old_func_id).ok_or_else(|| {
                GlslError::new(
                    ErrorCode::E0400,
                    alloc::format!(
                        "Old FuncId {} not found in old_func_id_map",
                        old_func_id.as_u32()
                    ),
                )
            })?;
            let new_func_id = func_id_map.get(func_name).ok_or_else(|| {
                GlslError::new(
                    ErrorCode::E0400,
                    alloc::format!("Function '{func_name}' not found in func_id_map"),
                )
            })?;

            // Create new UserExternalName with the NEW FuncId
            let new_user_name = cranelift_codegen::ir::UserExternalName {
                namespace: user_name.namespace,
                index: new_func_id.as_u32(),
            };
            let new_user_ref = builder.func.declare_imported_user_function(new_user_name);
            ExternalName::User(new_user_ref)
        }
        _ => old_ext_func.name.clone(),
    };

    let new_ext_func = ExtFuncData {
        name: new_name,
        signature: new_sig_ref,
        colocated: old_ext_func.colocated,
    };

    let new_func_ref = builder.func.import_function(new_ext_func);
    ext_func_map.insert(old_func_ref, new_func_ref);

    Ok(new_func_ref)
}

/// Convert Call instruction.
pub(crate) fn convert_call(
    old_func: &Function,
    old_inst: Inst,
    builder: &mut FunctionBuilder,
    value_map: &mut HashMap<Value, Value>,
    ext_func_map: &mut HashMap<FuncRef, FuncRef>,
    sig_map: &mut HashMap<SigRef, SigRef>,
    format: FixedPointFormat,
    func_id_map: &HashMap<String, FuncId>,
    old_func_id_map: &HashMap<FuncId, String>,
    pointer_type: cranelift_codegen::ir::Type,
) -> Result<(), GlslError> {
    let inst_data = &old_func.dfg.insts[old_inst];

    if let InstructionData::Call {
        opcode: _,
        func_ref,
        args,
    } = inst_data
    {
        // Check if this is a TestCase or User function that should be converted to a builtin
        let old_ext_func = &old_func.dfg.ext_funcs[*func_ref];

        // Try to get the function name - could be TestCase or User
        let func_name_opt: Option<&str> = match &old_ext_func.name {
            ExternalName::TestCase(testcase_name) => core::str::from_utf8(testcase_name.raw()).ok(),
            ExternalName::User(user_ref) => {
                // Look up function name from old_func_id_map
                if let Some(user_name) = old_func.params.user_named_funcs().get(*user_ref) {
                    let old_func_id = cranelift_module::FuncId::from_u32(user_name.index);
                    old_func_id_map.get(&old_func_id).map(|s| s.as_str())
                } else {
                    None
                }
            }
            _ => None,
        };

        let new_func_ref = if let Some(func_name) = func_name_opt {
            // Check if this is a function that should be converted inline (fract, sign, isinf, isnan)
            let old_args = args.as_slice(&old_func.dfg.value_lists);

            // Handle inline conversions for simple functions
            if func_name == "fractf" || func_name == "__lp_fract" {
                if old_args.len() != 1 {
                    return Err(GlslError::new(
                        ErrorCode::E0400,
                        format!("Expected 1 argument for fract, got {}", old_args.len()),
                    ));
                }
                let arg = map_value(old_func, value_map, old_args[0])?;
                // fract(x) = x - floor(x)
                let target_type = format.cranelift_type();
                let shift_amount = format.shift_amount();
                let shift_const = builder.ins().iconst(target_type, shift_amount);
                let rounded = builder.ins().sshr(arg, shift_const);
                let floored = builder.ins().ishl(rounded, shift_const);
                let result = builder.ins().isub(arg, floored);
                let old_result = get_first_result(old_func, old_inst);
                value_map.insert(old_result, result);
                return Ok(());
            } else if func_name == "signf" || func_name == "__lp_sign" {
                if old_args.len() != 1 {
                    return Err(GlslError::new(
                        ErrorCode::E0400,
                        format!("Expected 1 argument for sign, got {}", old_args.len()),
                    ));
                }
                use cranelift_codegen::ir::condcodes::IntCC;
                let arg = map_value(old_func, value_map, old_args[0])?;
                let target_type = format.cranelift_type();
                let zero = builder.ins().iconst(target_type, 0);
                let one = builder.ins().iconst(target_type, 0x00010000i64); // 1.0 in fixed16x16
                let minus_one = builder.ins().iconst(target_type, -0x00010000i64); // -1.0 in fixed16x16
                let gt_zero = builder.ins().icmp(IntCC::SignedGreaterThan, arg, zero);
                let lt_zero = builder.ins().icmp(IntCC::SignedLessThan, arg, zero);
                let temp = builder.ins().select(gt_zero, one, zero);
                let result = builder.ins().select(lt_zero, minus_one, temp);
                let old_result = get_first_result(old_func, old_inst);
                value_map.insert(old_result, result);
                return Ok(());
            } else if func_name == "isinff" || func_name == "__lp_isinf" {
                if old_args.len() != 1 {
                    return Err(GlslError::new(
                        ErrorCode::E0400,
                        format!("Expected 1 argument for isinf, got {}", old_args.len()),
                    ));
                }
                use cranelift_codegen::ir::condcodes::IntCC;
                // After transform, val is i32 (fixed-point)
                // Check if val equals MAX_FIXED (0x7FFF_FFFF) or MIN_FIXED (i32::MIN)
                // These are sentinel values from division by zero
                let arg = map_value(old_func, value_map, old_args[0])?;
                let target_type = format.cranelift_type();
                let max_fixed = builder.ins().iconst(target_type, 0x7FFF_FFFFi64);
                let min_fixed = builder.ins().iconst(target_type, i32::MIN as i64);
                let _zero_i8 = builder.ins().iconst(types::I8, 0);
                let _one_i8 = builder.ins().iconst(types::I8, 1);
                let is_max = builder.ins().icmp(IntCC::Equal, arg, max_fixed);
                let is_min = builder.ins().icmp(IntCC::Equal, arg, min_fixed);
                let is_inf = builder.ins().bor(is_max, is_min);
                let old_result = get_first_result(old_func, old_inst);
                value_map.insert(old_result, is_inf);
                return Ok(());
            } else if func_name == "isnanf" || func_name == "__lp_isnan" {
                if old_args.len() != 1 {
                    return Err(GlslError::new(
                        ErrorCode::E0400,
                        format!("Expected 1 argument for isnan, got {}", old_args.len()),
                    ));
                }
                // Fixed-point doesn't have NaN, so isnan always returns false
                let false_val = builder.ins().iconst(types::I8, 0);
                let old_result = get_first_result(old_func, old_inst);
                value_map.insert(old_result, false_val);
                return Ok(());
            } else if func_name == "isinff" || func_name == "__lp_isinf" {
                if old_args.len() != 1 {
                    return Err(GlslError::new(
                        ErrorCode::E0400,
                        format!("Expected 1 argument for isinf, got {}", old_args.len()),
                    ));
                }
                use cranelift_codegen::ir::condcodes::IntCC;
                // After transform, val is i32 (fixed-point)
                // Check if val equals MAX_FIXED (0x7FFF_FFFF) or MIN_FIXED (i32::MIN)
                // These are sentinel values from division by zero
                let arg = map_value(old_func, value_map, old_args[0])?;
                let target_type = format.cranelift_type();
                let max_fixed = builder.ins().iconst(target_type, 0x7FFF_FFFFi64);
                let min_fixed = builder.ins().iconst(target_type, i32::MIN as i64);
                let _zero_i8 = builder.ins().iconst(types::I8, 0);
                let is_max = builder.ins().icmp(IntCC::Equal, arg, max_fixed);
                let is_min = builder.ins().icmp(IntCC::Equal, arg, min_fixed);
                let is_inf = builder.ins().bor(is_max, is_min);
                let old_result = get_first_result(old_func, old_inst);
                value_map.insert(old_result, is_inf);
                return Ok(());
            }

            // Check if this is a function that should be converted to a builtin
            let old_args = args.as_slice(&old_func.dfg.value_lists);

            // Try LPFX function lookup chain: name -> BuiltinId -> LpfxFn -> q32_impl
            let q32_builtin_id =
                if let Some(builtin_id) = BuiltinId::builtin_id_from_name(func_name) {
                    // Check if this is an LPFX function
                    if let Some(lpfx_fn) = find_lpfx_fn_by_builtin_id(builtin_id) {
                        // Extract q32_impl from LpfxFn
                        match &lpfx_fn.impls {
                            crate::frontend::semantic::lpfx::lpfx_fn::LpfxFnImpl::Decimal {
                                q32_impl,
                                ..
                            } => Some(*q32_impl),
                            crate::frontend::semantic::lpfx::lpfx_fn::LpfxFnImpl::NonDecimal(_) => {
                                // Non-decimal functions don't need conversion
                                None
                            }
                        }
                    } else {
                        // Not an LPFX function, fall back to regular q32 lookup
                        map_testcase_to_builtin(func_name, old_args.len())
                    }
                } else {
                    // Unknown builtin name, try regular q32 lookup
                    map_testcase_to_builtin(func_name, old_args.len())
                };

            if let Some(builtin_id) = q32_builtin_id {
                // Convert to builtin call (similar to convert_sqrt)

                // Map all arguments
                let mapped_args: Vec<Value> = old_args
                    .iter()
                    .map(|&v| map_value(old_func, value_map, v))
                    .collect::<Result<Vec<_>, _>>()?;

                // Get FuncId for the builtin from func_id_map
                let builtin_name = builtin_id.name();
                let func_id = func_id_map.get(builtin_name).ok_or_else(|| {
                    GlslError::new(
                        ErrorCode::E0400,
                        format!("Builtin function '{builtin_name}' not found in func_id_map"),
                    )
                })?;

                // Use the builtin's signature - it knows about result pointer and correct parameter types
                // Use the ISA's pointer type (not hardcoded I32) to support both 32-bit and 64-bit targets
                let sig = builtin_id.signature(pointer_type);
                let sig_ref = builder.func.import_signature(sig);

                // Create UserExternalName with the FuncId
                let user_name = UserExternalName {
                    namespace: 0, // Use namespace 0 for builtins
                    index: func_id.as_u32(),
                };
                let user_ref = builder.func.declare_imported_user_function(user_name);
                let ext_name = ExternalName::User(user_ref);

                let ext_func = ExtFuncData {
                    name: ext_name,
                    signature: sig_ref,
                    // Builtin functions are external and may be far away, so they cannot be colocated.
                    // This prevents ARM64 call relocation range issues (colocated uses ±128MB range).
                    colocated: false,
                };
                let builtin_func_ref = builder.func.import_function(ext_func);

                // Handle return values based on whether this uses a result pointer
                let sig = builtin_id.signature(pointer_type);
                let uses_result_ptr = sig.returns.is_empty(); // Functions with result pointer return void

                // Call the builtin function with mapped arguments
                let call_inst = builder.ins().call(builtin_func_ref, &mapped_args);

                if uses_result_ptr {
                    // Functions with result pointer return void - the call doesn't produce results
                    // The old call instruction's results come from loads from the buffer that happen after the call
                    // Those loads are separate instructions that will be processed by the transform
                    // The buffer pointer is already in mapped_args[0], so the loads will read from the correct buffer
                    // We don't need to map any results here - the loads handle that
                } else {
                    // Scalar return: extract from call results
                    let results = builder.inst_results(call_inst);
                    if results.is_empty() {
                        return Err(GlslError::new(
                            ErrorCode::E0400,
                            "Expected return value from builtin function, but got none",
                        ));
                    }
                    if results.len() != 1 {
                        return Err(GlslError::new(
                            ErrorCode::E0400,
                            format!(
                                "Expected 1 return value from builtin function, got {}",
                                results.len()
                            ),
                        ));
                    }
                    let old_result = get_first_result(old_func, old_inst);
                    value_map.insert(old_result, results[0]);
                }

                return Ok(());
            } else {
                // Not a math function, handle as regular function call
                // Get the old signature and transform it
                let old_sig_ref = old_ext_func.signature;
                let old_sig = &old_func.dfg.signatures[old_sig_ref];
                let new_sig = convert_signature(old_sig, format);

                // Import signature into current function's context
                let new_sig_ref = if let Some(&mapped_sig_ref) = sig_map.get(&old_sig_ref) {
                    mapped_sig_ref
                } else {
                    let imported_sig_ref = builder.func.import_signature(new_sig);
                    sig_map.insert(old_sig_ref, imported_sig_ref);
                    imported_sig_ref
                };

                // Handle TestCase vs User external names
                let new_name = match &old_ext_func.name {
                    ExternalName::TestCase(testcase_name) => {
                        // Convert TestCase name to User name for ObjectModule compatibility
                        // ObjectModule doesn't support TestCase names in relocations (unimplemented!)
                        // Extract function name from TestCase format (%name -> name)
                        let func_name_str =
                            core::str::from_utf8(testcase_name.raw()).map_err(|e| {
                                GlslError::new(
                                    ErrorCode::E0400,
                                    format!("Invalid TestCase name encoding: {e}"),
                                )
                            })?;
                        // Convert TestCase name to builtin name if it's a math function
                        // func_id_map contains builtin names (e.g., "__lp_q32_atan2"), not TestCase names (e.g., "atan2f")
                        // Note: We can't determine arg_count here, so we try to match without it
                        // This is a fallback for non-overloaded functions
                        let lookup_name =
                            if let Some(builtin_id) = map_testcase_to_builtin(func_name_str, 0) {
                                builtin_id.name()
                            } else {
                                func_name_str
                            };
                        // Look up the new FuncId for this function name
                        let new_func_id = func_id_map.get(lookup_name).ok_or_else(|| {
                            GlslError::new(
                                ErrorCode::E0400,
                                format!("Function '{func_name_str}' not found in func_id_map"),
                            )
                        })?;
                        // Create UserExternalName with the new FuncId
                        let new_user_name = cranelift_codegen::ir::UserExternalName {
                            namespace: 0,
                            index: new_func_id.as_u32(),
                        };
                        let new_user_ref =
                            builder.func.declare_imported_user_function(new_user_name);
                        ExternalName::User(new_user_ref)
                    }
                    ExternalName::User(old_user_ref) => {
                        // Map User function reference to new FuncId
                        let user_name = old_func
                            .params
                            .user_named_funcs()
                            .get(*old_user_ref)
                            .ok_or_else(|| {
                                GlslError::new(
                                    ErrorCode::E0400,
                                    format!(
                                        "UserExternalNameRef {old_user_ref} not found in old function's user_named_funcs"
                                    ),
                                )
                            })?;
                        let old_func_id = cranelift_module::FuncId::from_u32(user_name.index);
                        // Check if this is a builtin function (not in old_func_id_map)
                        let new_func_id = if let Some(func_name) = old_func_id_map.get(&old_func_id)
                        {
                            // Regular user function - remap through old_func_id_map
                            func_id_map.get(func_name).ok_or_else(|| {
                                GlslError::new(
                                    ErrorCode::E0400,
                                    format!("Function '{func_name}' not found in func_id_map"),
                                )
                            })?
                        } else {
                            // Builtin function - FuncId should be stable, check if it exists in func_id_map
                            // Try to find the builtin name by checking all builtins
                            use crate::backend::builtins::registry::BuiltinId;
                            let mut found_builtin_id = None;
                            for builtin in BuiltinId::all() {
                                if let Some(&func_id) = func_id_map.get(builtin.name()) {
                                    if func_id == old_func_id {
                                        found_builtin_id =
                                            Some(func_id_map.get(builtin.name()).unwrap());
                                        break;
                                    }
                                }
                            }
                            found_builtin_id.ok_or_else(|| {
                                GlslError::new(
                                    ErrorCode::E0400,
                                    format!(
                                        "Old FuncId {} not found in old_func_id_map or builtin registry",
                                        old_func_id.as_u32()
                                    ),
                                )
                            })?
                        };
                        let new_user_name = UserExternalName {
                            namespace: user_name.namespace,
                            index: new_func_id.as_u32(),
                        };
                        let new_user_ref =
                            builder.func.declare_imported_user_function(new_user_name);
                        ExternalName::User(new_user_ref)
                    }
                    _ => old_ext_func.name.clone(),
                };

                let new_ext_func = ExtFuncData {
                    name: new_name,
                    signature: new_sig_ref,
                    colocated: old_ext_func.colocated,
                };

                builder.func.import_function(new_ext_func)
            }
        } else {
            // Use existing logic for external functions (with caching)
            map_external_function(
                old_func,
                *func_ref,
                builder,
                ext_func_map,
                sig_map,
                format,
                func_id_map,
                old_func_id_map,
            )?
        };

        let old_args = args.as_slice(&old_func.dfg.value_lists);
        let new_args: Vec<Value> = old_args
            .iter()
            .map(|&v| map_value(old_func, value_map, v))
            .collect::<Result<Vec<_>, _>>()?;

        let call_inst = builder.ins().call(new_func_ref, &new_args);

        let old_results: Vec<Value> = old_func.dfg.inst_results(old_inst).to_vec();
        let new_results: Vec<Value> = builder.inst_results(call_inst).to_vec();

        if old_results.len() != new_results.len() {
            return Err(GlslError::new(
                ErrorCode::E0301,
                alloc::format!(
                    "Call return value count mismatch: old={}, new={}",
                    old_results.len(),
                    new_results.len()
                ),
            ));
        }

        for (old_result, new_result) in old_results.iter().zip(new_results.iter()) {
            value_map.insert(*old_result, *new_result);
        }
    } else {
        return Err(GlslError::new(
            ErrorCode::E0301,
            alloc::format!("Call instruction has unexpected format: {inst_data:?}"),
        ));
    }

    Ok(())
}

/// Convert CallIndirect instruction.
pub(crate) fn convert_call_indirect(
    old_func: &Function,
    old_inst: Inst,
    builder: &mut FunctionBuilder,
    value_map: &mut HashMap<Value, Value>,
    sig_map: &mut HashMap<SigRef, SigRef>,
    format: FixedPointFormat,
) -> Result<(), GlslError> {
    let inst_data = &old_func.dfg.insts[old_inst];

    if let InstructionData::CallIndirect {
        opcode: _,
        sig_ref,
        args,
    } = inst_data
    {
        let new_sig_ref = if let Some(&mapped_sig_ref) = sig_map.get(sig_ref) {
            mapped_sig_ref
        } else {
            let old_sig = &old_func.dfg.signatures[*sig_ref];
            let new_sig = convert_signature(old_sig, format);
            let new_sig_ref = builder.func.import_signature(new_sig);
            sig_map.insert(*sig_ref, new_sig_ref);
            new_sig_ref
        };

        let old_args = args.as_slice(&old_func.dfg.value_lists);
        let func_addr = map_value(old_func, value_map, old_args[0])?;
        let call_args: Vec<Value> = old_args[1..]
            .iter()
            .map(|&v| map_value(old_func, value_map, v))
            .collect::<Result<Vec<_>, _>>()?;

        let call_inst = builder
            .ins()
            .call_indirect(new_sig_ref, func_addr, &call_args);

        let old_results: Vec<Value> = old_func.dfg.inst_results(old_inst).to_vec();
        let new_results: Vec<Value> = builder.inst_results(call_inst).to_vec();

        if old_results.len() != new_results.len() {
            return Err(GlslError::new(
                ErrorCode::E0301,
                alloc::format!(
                    "CallIndirect return value count mismatch: old={}, new={}",
                    old_results.len(),
                    new_results.len()
                ),
            ));
        }

        for (old_result, new_result) in old_results.iter().zip(new_results.iter()) {
            value_map.insert(*old_result, *new_result);
        }
    } else {
        return Err(GlslError::new(
            ErrorCode::E0301,
            alloc::format!("CallIndirect instruction has unexpected format: {inst_data:?}"),
        ));
    }

    Ok(())
}

#[cfg(test)]
#[cfg(feature = "std")]
mod tests {
    #[cfg(feature = "emulator")]
    use crate::backend::transform::q32::q32_test_util;

    /// Test function calls: add(f32, f32) -> f32
    #[test]
    #[cfg(feature = "emulator")]
    fn test_q32_call() {
        let clif = r#"
function %add(f32, f32) -> f32 system_v {
block0(v0: f32, v1: f32):
    v2 = fadd v0, v1
    return v2
}

function %main() -> f32 system_v {
    sig0 = (f32, f32) -> f32 system_v
    fn0 = colocated %add sig0

block0:
    v0 = f32const 0x1.8p0
    v1 = f32const 0x1.4p1
    v2 = call fn0(v0, v1)
    return v2
}
"#;
        q32_test_util::run_q32_test(clif, 4.0);
    }
}
