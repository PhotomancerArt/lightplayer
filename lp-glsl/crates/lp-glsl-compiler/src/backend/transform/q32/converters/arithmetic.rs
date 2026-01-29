//! Arithmetic operation conversion functions.

use crate::backend::transform::q32::converters::{
    create_zero_const, extract_binary_operands, extract_unary_operand, get_first_result,
    map_operand,
};
use crate::backend::transform::q32::types::FixedPointFormat;
use crate::error::GlslError;
use cranelift_codegen::ir::{Function, Inst, InstBuilder, condcodes::IntCC, types};
use cranelift_frontend::FunctionBuilder;
use hashbrown::HashMap;

use alloc::format;

/// Convert Fadd to fixed-point addition by calling __lp_q32_add builtin.
pub(crate) fn convert_fadd(
    old_func: &Function,
    old_inst: Inst,
    builder: &mut FunctionBuilder,
    value_map: &mut HashMap<cranelift_codegen::ir::Value, cranelift_codegen::ir::Value>,
    _format: FixedPointFormat,
    func_id_map: &HashMap<alloc::string::String, cranelift_module::FuncId>,
) -> Result<(), GlslError> {
    use cranelift_codegen::ir::{AbiParam, ExtFuncData, ExternalName, Signature, UserExternalName};
    use cranelift_codegen::isa::CallConv;

    let (arg1_old, arg2_old) = extract_binary_operands(old_func, old_inst)?;
    let arg1 = map_operand(old_func, value_map, arg1_old)?;
    let arg2 = map_operand(old_func, value_map, arg2_old)?;

    // Get FuncId for __lp_q32_add from func_id_map
    let builtin_name = "__lp_q32_add";
    let func_id = func_id_map.get(builtin_name).ok_or_else(|| {
        GlslError::new(
            crate::error::ErrorCode::E0400,
            format!("Builtin function '{builtin_name}' not found in func_id_map"),
        )
    })?;

    // Create signature for __lp_q32_add: (i32, i32) -> i32
    let mut sig = Signature::new(CallConv::SystemV);
    sig.params.push(AbiParam::new(types::I32));
    sig.params.push(AbiParam::new(types::I32));
    sig.returns.push(AbiParam::new(types::I32));
    let sig_ref = builder.func.import_signature(sig);

    // Create UserExternalName with the FuncId
    let user_name = UserExternalName {
        namespace: 0, // Use namespace 0 for builtins
        index: func_id.as_u32(),
    };
    let user_ref = builder.func.declare_imported_user_function(user_name);
    let ext_name = ExternalName::User(user_ref);

    // Builtin functions are external and may be far away, so they cannot be colocated.
    // This prevents ARM64 call relocation range issues (colocated uses ±128MB range).
    // For JIT mode, function pointers are resolved at runtime via symbol_lookup_fn.
    // For emulator mode, the linker will handle the relocation appropriately.
    let ext_func = ExtFuncData {
        name: ext_name,
        signature: sig_ref,
        colocated: false,
    };
    let add_func_ref = builder.func.import_function(ext_func);

    // Call __lp_q32_add with the mapped arguments
    let call_result = builder.ins().call(add_func_ref, &[arg1, arg2]);
    let result = builder.inst_results(call_result)[0];

    let old_result = get_first_result(old_func, old_inst);
    value_map.insert(old_result, result);

    Ok(())
}

/// Convert Fsub to fixed-point subtraction by calling __lp_q32_sub builtin.
pub(crate) fn convert_fsub(
    old_func: &Function,
    old_inst: Inst,
    builder: &mut FunctionBuilder,
    value_map: &mut HashMap<cranelift_codegen::ir::Value, cranelift_codegen::ir::Value>,
    _format: FixedPointFormat,
    func_id_map: &HashMap<alloc::string::String, cranelift_module::FuncId>,
) -> Result<(), GlslError> {
    use cranelift_codegen::ir::{AbiParam, ExtFuncData, ExternalName, Signature, UserExternalName};
    use cranelift_codegen::isa::CallConv;

    let (arg1_old, arg2_old) = extract_binary_operands(old_func, old_inst)?;
    let arg1 = map_operand(old_func, value_map, arg1_old)?;
    let arg2 = map_operand(old_func, value_map, arg2_old)?;

    // Get FuncId for __lp_q32_sub from func_id_map
    let builtin_name = "__lp_q32_sub";
    let func_id = func_id_map.get(builtin_name).ok_or_else(|| {
        GlslError::new(
            crate::error::ErrorCode::E0400,
            format!("Builtin function '{builtin_name}' not found in func_id_map"),
        )
    })?;

    // Create signature for __lp_q32_sub: (i32, i32) -> i32
    let mut sig = Signature::new(CallConv::SystemV);
    sig.params.push(AbiParam::new(types::I32));
    sig.params.push(AbiParam::new(types::I32));
    sig.returns.push(AbiParam::new(types::I32));
    let sig_ref = builder.func.import_signature(sig);

    // Create UserExternalName with the FuncId
    let user_name = UserExternalName {
        namespace: 0, // Use namespace 0 for builtins
        index: func_id.as_u32(),
    };
    let user_ref = builder.func.declare_imported_user_function(user_name);
    let ext_name = ExternalName::User(user_ref);

    // Builtin functions are external and may be far away, so they cannot be colocated.
    // This prevents ARM64 call relocation range issues (colocated uses ±128MB range).
    // For JIT mode, function pointers are resolved at runtime via symbol_lookup_fn.
    // For emulator mode, the linker will handle the relocation appropriately.
    let ext_func = ExtFuncData {
        name: ext_name,
        signature: sig_ref,
        colocated: false,
    };
    let sub_func_ref = builder.func.import_function(ext_func);

    // Call __lp_q32_sub with the mapped arguments
    let call_result = builder.ins().call(sub_func_ref, &[arg1, arg2]);
    let result = builder.inst_results(call_result)[0];

    let old_result = get_first_result(old_func, old_inst);
    value_map.insert(old_result, result);

    Ok(())
}

/// Convert Fmul to fixed-point multiplication by calling __lp_q32_mul builtin.
pub(crate) fn convert_fmul(
    old_func: &Function,
    old_inst: Inst,
    builder: &mut FunctionBuilder,
    value_map: &mut HashMap<cranelift_codegen::ir::Value, cranelift_codegen::ir::Value>,
    _format: FixedPointFormat,
    func_id_map: &HashMap<alloc::string::String, cranelift_module::FuncId>,
) -> Result<(), GlslError> {
    use cranelift_codegen::ir::{AbiParam, ExtFuncData, ExternalName, Signature, UserExternalName};
    use cranelift_codegen::isa::CallConv;

    let (arg1_old, arg2_old) = extract_binary_operands(old_func, old_inst)?;
    let arg1 = map_operand(old_func, value_map, arg1_old)?;
    let arg2 = map_operand(old_func, value_map, arg2_old)?;

    // Get FuncId for __lp_q32_mul from func_id_map
    let builtin_name = "__lp_q32_mul";
    let func_id = func_id_map.get(builtin_name).ok_or_else(|| {
        GlslError::new(
            crate::error::ErrorCode::E0400,
            format!("Builtin function '{builtin_name}' not found in func_id_map"),
        )
    })?;

    // Create signature for __lp_q32_mul: (i32, i32) -> i32
    let mut sig = Signature::new(CallConv::SystemV);
    sig.params.push(AbiParam::new(types::I32));
    sig.params.push(AbiParam::new(types::I32));
    sig.returns.push(AbiParam::new(types::I32));
    let sig_ref = builder.func.import_signature(sig);

    // Create UserExternalName with the FuncId
    let user_name = UserExternalName {
        namespace: 0, // Use namespace 0 for builtins
        index: func_id.as_u32(),
    };
    let user_ref = builder.func.declare_imported_user_function(user_name);
    let ext_name = ExternalName::User(user_ref);

    // Builtin functions are external and may be far away, so they cannot be colocated.
    // This prevents ARM64 call relocation range issues (colocated uses ±128MB range).
    // For JIT mode, function pointers are resolved at runtime via symbol_lookup_fn.
    // For emulator mode, the linker will handle the relocation appropriately.
    let ext_func = ExtFuncData {
        name: ext_name,
        signature: sig_ref,
        colocated: false,
    };
    let mul_func_ref = builder.func.import_function(ext_func);

    // Call __lp_q32_mul with the mapped arguments
    let call_result = builder.ins().call(mul_func_ref, &[arg1, arg2]);
    let result = builder.inst_results(call_result)[0];

    let old_result = get_first_result(old_func, old_inst);
    value_map.insert(old_result, result);

    Ok(())
}

/// Convert Fdiv to fixed-point division by calling __lp_q32_div builtin.
pub(crate) fn convert_fdiv(
    old_func: &Function,
    old_inst: Inst,
    builder: &mut FunctionBuilder,
    value_map: &mut HashMap<cranelift_codegen::ir::Value, cranelift_codegen::ir::Value>,
    _format: FixedPointFormat,
    func_id_map: &HashMap<alloc::string::String, cranelift_module::FuncId>,
) -> Result<(), GlslError> {
    use cranelift_codegen::ir::{AbiParam, ExtFuncData, ExternalName, Signature, UserExternalName};
    use cranelift_codegen::isa::CallConv;

    let (arg1_old, arg2_old) = extract_binary_operands(old_func, old_inst)?;
    let arg1 = map_operand(old_func, value_map, arg1_old)?;
    let arg2 = map_operand(old_func, value_map, arg2_old)?;

    // Get FuncId for __lp_q32_div from func_id_map
    let builtin_name = "__lp_q32_div";
    let func_id = func_id_map.get(builtin_name).ok_or_else(|| {
        GlslError::new(
            crate::error::ErrorCode::E0400,
            format!("Builtin function '{builtin_name}' not found in func_id_map"),
        )
    })?;

    // Create signature for __lp_q32_div: (i32, i32) -> i32
    let mut sig = Signature::new(CallConv::SystemV);
    sig.params.push(AbiParam::new(types::I32));
    sig.params.push(AbiParam::new(types::I32));
    sig.returns.push(AbiParam::new(types::I32));
    let sig_ref = builder.func.import_signature(sig);

    // Create UserExternalName with the FuncId
    let user_name = UserExternalName {
        namespace: 0, // Use namespace 0 for builtins
        index: func_id.as_u32(),
    };
    let user_ref = builder.func.declare_imported_user_function(user_name);
    let ext_name = ExternalName::User(user_ref);

    // Builtin functions are external and may be far away, so they cannot be colocated.
    // This prevents ARM64 call relocation range issues (colocated uses ±128MB range).
    // For JIT mode, function pointers are resolved at runtime via symbol_lookup_fn.
    // For emulator mode, the linker will handle the relocation appropriately.
    let ext_func = ExtFuncData {
        name: ext_name,
        signature: sig_ref,
        colocated: false,
    };
    let div_func_ref = builder.func.import_function(ext_func);

    // Call __lp_q32_div with the mapped arguments
    let call_result = builder.ins().call(div_func_ref, &[arg1, arg2]);
    let result = builder.inst_results(call_result)[0];

    let old_result = get_first_result(old_func, old_inst);
    value_map.insert(old_result, result);

    Ok(())
}

/// Convert Fneg to fixed-point negation
pub(crate) fn convert_fneg(
    old_func: &Function,
    old_inst: Inst,
    builder: &mut FunctionBuilder,
    value_map: &mut HashMap<cranelift_codegen::ir::Value, cranelift_codegen::ir::Value>,
    _format: FixedPointFormat,
) -> Result<(), GlslError> {
    let arg = extract_unary_operand(old_func, old_inst)?;
    let mapped_arg = map_operand(old_func, value_map, arg)?;

    let result = builder.ins().ineg(mapped_arg);

    let old_result = get_first_result(old_func, old_inst);
    value_map.insert(old_result, result);

    Ok(())
}

/// Convert Fabs using conditional select
pub(crate) fn convert_fabs(
    old_func: &Function,
    old_inst: Inst,
    builder: &mut FunctionBuilder,
    value_map: &mut HashMap<cranelift_codegen::ir::Value, cranelift_codegen::ir::Value>,
    format: FixedPointFormat,
) -> Result<(), GlslError> {
    let arg = extract_unary_operand(old_func, old_inst)?;
    let mapped_arg = map_operand(old_func, value_map, arg)?;

    // Absolute value: if (arg < 0) then -arg else arg
    let zero = create_zero_const(builder, format);
    let is_negative = builder.ins().icmp(IntCC::SignedLessThan, mapped_arg, zero);
    let negated = builder.ins().ineg(mapped_arg);
    let result = builder.ins().select(is_negative, negated, mapped_arg);

    let old_result = get_first_result(old_func, old_inst);
    value_map.insert(old_result, result);

    Ok(())
}

#[cfg(test)]
#[cfg(feature = "std")]
mod tests {
    #[cfg(feature = "emulator")]
    use crate::backend::transform::q32::q32_test_util;

    /// Test fadd: addition
    #[test]
    #[cfg(feature = "emulator")]
    fn test_q32_fadd() {
        // Use proper hex scientific notation: 0x1.8p-1 = 0.75, 0x1.8p1 = 3.0
        let clif = r#"
function %main() -> f32 system_v {
block0:
    v0 = f32const 0x1.8p1
    v1 = f32const 0x1.8p-1
    v2 = fadd v0, v1
    return v2
}
"#;
        q32_test_util::run_q32_test(clif, 3.75);
    }

    /// Test fsub: subtraction
    #[test]
    #[cfg(feature = "emulator")]
    fn test_q32_fsub() {
        let clif = r#"
function %main() -> f32 system_v {
block0:
    v0 = f32const 0x1.4p2
    v1 = f32const 0x1.4p1
    v2 = fsub v0, v1
    return v2
}
"#;
        q32_test_util::run_q32_test(clif, 2.5);
    }

    /// Test fmul: multiplication
    #[test]
    #[cfg(feature = "emulator")]
    fn test_q32_fmul() {
        let clif = r#"
function %main() -> f32 system_v {
block0:
    v0 = f32const 0x1.0p1
    v1 = f32const 0x1.8p1
    v2 = fmul v0, v1
    return v2
}
"#;
        q32_test_util::run_q32_test(clif, 6.0);
    }

    /// Test fdiv: division
    ///
    /// NOTE: This test is currently ignored due to a known issue with the division algorithm.
    /// The old backend has the same algorithm and may have the same bug. We'll fix this separately.
    #[test]
    #[cfg(feature = "emulator")]
    fn test_q32_fdiv() {
        let clif = r#"
function %main() -> f32 system_v {
block0:
    v0 = f32const 0x1.4p3
    v1 = f32const 0x1.4p1
    v2 = fdiv v0, v1
    return v2
}
"#;
        q32_test_util::run_q32_test(clif, 4.0);
    }

    /// Test fneg: negation
    #[test]
    #[cfg(feature = "emulator")]
    fn test_q32_fneg() {
        let clif = r#"
function %main() -> f32 system_v {
block0:
    v0 = f32const 0x1.4p1
    v1 = fneg v0
    return v1
}
"#;
        q32_test_util::run_q32_test(clif, -2.5);
    }

    /// Test fabs: absolute value
    #[test]
    #[cfg(feature = "emulator")]
    fn test_q32_fabs() {
        // Test with negative value
        let clif = r#"
function %main() -> f32 system_v {
block0:
    v0 = f32const -0x1.4p1
    v1 = fabs v0
    return v1
}
"#;
        q32_test_util::run_q32_test(clif, 2.5);
    }

    /// Test fabs: absolute value with positive value
    #[test]
    #[cfg(feature = "emulator")]
    fn test_q32_fabs_positive() {
        let clif = r#"
function %main() -> f32 system_v {
block0:
    v0 = f32const 0x1.4p1
    v1 = fabs v0
    return v1
}
"#;
        q32_test_util::run_q32_test(clif, 2.5);
    }
}
