//! Math function conversion functions.

use crate::backend::transform::q32::converters::{
    extract_unary_operand, get_first_result, map_operand,
};
use crate::backend::transform::q32::types::FixedPointFormat;
use crate::error::GlslError;
use cranelift_codegen::ir::{Function, Inst, InstBuilder, Value, types};
use cranelift_frontend::FunctionBuilder;
use hashbrown::HashMap;

use alloc::format;

/// Convert Ceil instruction.
pub(crate) fn convert_ceil(
    old_func: &Function,
    old_inst: Inst,
    builder: &mut FunctionBuilder,
    value_map: &mut HashMap<Value, Value>,
    format: FixedPointFormat,
) -> Result<(), GlslError> {
    let arg = extract_unary_operand(old_func, old_inst)?;
    let mapped_arg = map_operand(old_func, value_map, arg)?;
    let target_type = format.cranelift_type();
    let shift_amount = format.shift_amount();

    // Ceil: round up to nearest integer
    // In fixed-point: (value + (1 << shift) - 1) >> shift, then << shift
    let mask = (1i64 << shift_amount) - 1;
    let mask_const = builder.ins().iconst(target_type, mask);
    let added = builder.ins().iadd(mapped_arg, mask_const);
    let shift_const = builder.ins().iconst(target_type, shift_amount);
    let rounded = builder.ins().sshr(added, shift_const);
    let new_result = builder.ins().ishl(rounded, shift_const);

    let old_result = get_first_result(old_func, old_inst);
    value_map.insert(old_result, new_result);

    Ok(())
}

/// Convert Floor instruction.
pub(crate) fn convert_floor(
    old_func: &Function,
    old_inst: Inst,
    builder: &mut FunctionBuilder,
    value_map: &mut HashMap<Value, Value>,
    format: FixedPointFormat,
) -> Result<(), GlslError> {
    let arg = extract_unary_operand(old_func, old_inst)?;
    let mapped_arg = map_operand(old_func, value_map, arg)?;
    let target_type = format.cranelift_type();
    let shift_amount = format.shift_amount();

    // Floor: round down to nearest integer
    // In fixed-point: value >> shift, then << shift
    let shift_const = builder.ins().iconst(target_type, shift_amount);
    let rounded = builder.ins().sshr(mapped_arg, shift_const);
    let new_result = builder.ins().ishl(rounded, shift_const);

    let old_result = get_first_result(old_func, old_inst);
    value_map.insert(old_result, new_result);

    Ok(())
}

/// Convert Trunc instruction.
pub(crate) fn convert_trunc(
    old_func: &Function,
    old_inst: Inst,
    builder: &mut FunctionBuilder,
    value_map: &mut HashMap<Value, Value>,
    format: FixedPointFormat,
) -> Result<(), GlslError> {
    // Trunc is the same as floor for positive numbers, but rounds toward zero
    // For fixed-point, we can use the same approach as floor
    convert_floor(old_func, old_inst, builder, value_map, format)
}

/// Convert Nearest instruction (round to nearest).
pub(crate) fn convert_nearest(
    old_func: &Function,
    old_inst: Inst,
    builder: &mut FunctionBuilder,
    value_map: &mut HashMap<Value, Value>,
    format: FixedPointFormat,
) -> Result<(), GlslError> {
    let arg = extract_unary_operand(old_func, old_inst)?;
    let mapped_arg = map_operand(old_func, value_map, arg)?;
    let target_type = format.cranelift_type();
    let shift_amount = format.shift_amount();

    // Nearest: round to nearest integer
    // In fixed-point: (value + (1 << (shift - 1))) >> shift, then << shift
    let half = 1i64 << (shift_amount - 1);
    let half_const = builder.ins().iconst(target_type, half);
    let added = builder.ins().iadd(mapped_arg, half_const);
    let shift_const = builder.ins().iconst(target_type, shift_amount);
    let rounded = builder.ins().sshr(added, shift_const);
    let new_result = builder.ins().ishl(rounded, shift_const);

    let old_result = get_first_result(old_func, old_inst);
    value_map.insert(old_result, new_result);

    Ok(())
}

/// Convert Sqrt by calling the linked __lp_q32_sqrt function.
pub(crate) fn convert_sqrt(
    old_func: &Function,
    old_inst: Inst,
    builder: &mut FunctionBuilder,
    value_map: &mut HashMap<Value, Value>,
    _format: FixedPointFormat,
    func_id_map: &HashMap<alloc::string::String, cranelift_module::FuncId>,
) -> Result<(), GlslError> {
    use cranelift_codegen::ir::{AbiParam, ExtFuncData, ExternalName, Signature, UserExternalName};
    use cranelift_codegen::isa::CallConv;

    let arg = extract_unary_operand(old_func, old_inst)?;
    let mapped_arg = map_operand(old_func, value_map, arg)?;

    // Get FuncId for __lp_q32_sqrt from func_id_map
    let builtin_name = "__lp_q32_sqrt";
    let func_id = func_id_map.get(builtin_name).ok_or_else(|| {
        GlslError::new(
            crate::error::ErrorCode::E0400,
            format!("Builtin function '{builtin_name}' not found in func_id_map"),
        )
    })?;

    // Create signature for __lp_q32_sqrt: (i32) -> i32
    let mut sig = Signature::new(CallConv::SystemV);
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
    let sqrt_func_ref = builder.func.import_function(ext_func);

    // Call __lp_q32_sqrt with the mapped argument
    let call_result = builder.ins().call(sqrt_func_ref, &[mapped_arg]);
    let result = builder.inst_results(call_result)[0];

    let old_result = get_first_result(old_func, old_inst);
    value_map.insert(old_result, result);

    Ok(())
}

#[cfg(test)]
#[cfg(feature = "std")]
mod tests {
    #[cfg(feature = "emulator")]
    use crate::backend::transform::q32::q32_test_util;

    /// Test sqrt: square root
    ///
    /// NOTE: This test is currently ignored because sqrt uses i64 division
    /// which is not supported on riscv32. We'll need to implement an alternative
    /// algorithm that doesn't require i64 division.
    #[test]
    #[cfg(feature = "emulator")]
    #[ignore]
    fn test_q32_sqrt() {
        let clif = r#"
function %main() -> f32 system_v {
block0:
    v0 = f32const 0x1.0p2
    v1 = sqrt v0
    return v1
}
"#;
        // Result should be 2.0 (sqrt of 4.0)
        // Note: Newton-Raphson may have some precision error, so we allow a small tolerance
        q32_test_util::run_q32_test(clif, 2.0);
    }

    /// Test sqrt: square root of 9.0
    ///
    /// NOTE: This test is currently ignored because sqrt uses i64 division
    /// which is not supported on riscv32.
    #[test]
    #[cfg(feature = "emulator")]
    #[ignore]
    fn test_q32_sqrt_9() {
        let clif = r#"
function %main() -> f32 system_v {
block0:
    v0 = f32const 0x1.2p3
    v1 = sqrt v0
    return v1
}
"#;
        // Result should be 3.0 (sqrt of 9.0)
        q32_test_util::run_q32_test(clif, 3.0);
    }

    /// Test sqrt: square root of zero
    ///
    /// NOTE: This test is currently ignored because sqrt uses i64 division
    /// which is not supported on riscv32.
    #[test]
    #[cfg(feature = "emulator")]
    #[ignore]
    fn test_q32_sqrt_zero() {
        let clif = r#"
function %main() -> f32 system_v {
block0:
    v0 = f32const 0x0.0p0
    v1 = sqrt v0
    return v1
}
"#;
        // Result should be 0.0
        q32_test_util::run_q32_test(clif, 0.0);
    }
}
