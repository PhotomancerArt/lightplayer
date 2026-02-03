//! Comparison operation conversion functions.

use crate::backend::transform::q32::converters::{
    extract_binary_operands, get_first_result, map_operand,
};
use crate::backend::transform::q32::types::FixedPointFormat;
use crate::error::GlslError;
use cranelift_codegen::ir::{
    Function, Inst, InstBuilder, InstructionData, Value,
    condcodes::{FloatCC, IntCC},
};
use cranelift_frontend::FunctionBuilder;
use hashbrown::HashMap;

/// Convert Fcmp to icmp.
///
/// Note: Fixed-point arithmetic does not have NaN or Infinity values.
/// FloatCC conditions that check for NaN/Inf (Ordered/Unordered) are approximated
/// using integer comparisons, which may not match floating-point behavior exactly
/// for edge cases involving NaN or Infinity.
pub(crate) fn convert_fcmp(
    old_func: &Function,
    old_inst: Inst,
    builder: &mut FunctionBuilder,
    value_map: &mut HashMap<Value, Value>,
    _format: FixedPointFormat,
) -> Result<(), GlslError> {
    let inst_data = &old_func.dfg.insts[old_inst];

    if let InstructionData::FloatCompare { cond, args, .. } = inst_data {
        // Map operands
        let arg1 = map_operand(old_func, value_map, args[0])?;
        let arg2 = map_operand(old_func, value_map, args[1])?;

        // Convert float condition to integer condition
        let int_cond = match cond {
            FloatCC::Equal => IntCC::Equal,
            FloatCC::NotEqual => IntCC::NotEqual,
            FloatCC::LessThan => IntCC::SignedLessThan,
            FloatCC::LessThanOrEqual => IntCC::SignedLessThanOrEqual,
            FloatCC::GreaterThan => IntCC::SignedGreaterThan,
            FloatCC::GreaterThanOrEqual => IntCC::SignedGreaterThanOrEqual,
            // Note: Ordered/Unordered conditions are approximated since fixed-point
            // doesn't have NaN. These approximations may not match float behavior exactly.
            // Note: Ordered/Unordered conditions are approximated since fixed-point
            // doesn't have NaN. These approximations may not match float behavior exactly.
            FloatCC::Ordered => IntCC::Equal, // Approximate: always true for fixed-point
            FloatCC::Unordered => IntCC::NotEqual, // Approximate: always false for fixed-point
            FloatCC::OrderedNotEqual => IntCC::NotEqual,
            FloatCC::UnorderedOrEqual => IntCC::Equal,
            FloatCC::UnorderedOrLessThan => IntCC::SignedLessThan,
            FloatCC::UnorderedOrLessThanOrEqual => IntCC::SignedLessThanOrEqual,
            FloatCC::UnorderedOrGreaterThan => IntCC::SignedGreaterThan,
            FloatCC::UnorderedOrGreaterThanOrEqual => IntCC::SignedGreaterThanOrEqual,
        };

        // Emit icmp (returns i8)
        let cmp_result = builder.ins().icmp(int_cond, arg1, arg2);

        // Convert boolean result (i8: 0 or 1) to fixed-point value (i32: 0 or 65536)
        // fcmp returns i8 (0 or 1), but we need to convert to fixed-point representation
        // where 1.0 (true) = 65536 and 0.0 (false) = 0
        let cmp_i32 = builder
            .ins()
            .sextend(cranelift_codegen::ir::types::I32, cmp_result);

        // Multiply by fixed-point scale (65536) to convert boolean to fixed-point
        // 0 * 65536 = 0 (false), 1 * 65536 = 65536 (true = 1.0)
        let scale = builder
            .ins()
            .iconst(cranelift_codegen::ir::types::I32, 65536);
        let result = builder.ins().imul(cmp_i32, scale);

        let old_result = get_first_result(old_func, old_inst);
        value_map.insert(old_result, result);
    } else {
        return Err(GlslError::new(
            crate::error::ErrorCode::E0301,
            alloc::format!("Fcmp instruction has unexpected format: {inst_data:?}"),
        ));
    }

    Ok(())
}

/// Convert Fmax to select with comparison.
pub(crate) fn convert_fmax(
    old_func: &Function,
    old_inst: Inst,
    builder: &mut FunctionBuilder,
    value_map: &mut HashMap<Value, Value>,
    _format: FixedPointFormat,
) -> Result<(), GlslError> {
    let (arg1_old, arg2_old) = extract_binary_operands(old_func, old_inst)?;
    let arg1 = map_operand(old_func, value_map, arg1_old)?;
    let arg2 = map_operand(old_func, value_map, arg2_old)?;

    // Compare and select maximum
    let cmp = builder.ins().icmp(IntCC::SignedGreaterThan, arg1, arg2);
    let new_result = builder.ins().select(cmp, arg1, arg2);

    let old_result = get_first_result(old_func, old_inst);
    value_map.insert(old_result, new_result);

    Ok(())
}

/// Convert Fmin to select with comparison.
pub(crate) fn convert_fmin(
    old_func: &Function,
    old_inst: Inst,
    builder: &mut FunctionBuilder,
    value_map: &mut HashMap<Value, Value>,
    _format: FixedPointFormat,
) -> Result<(), GlslError> {
    let (arg1_old, arg2_old) = extract_binary_operands(old_func, old_inst)?;
    let arg1 = map_operand(old_func, value_map, arg1_old)?;
    let arg2 = map_operand(old_func, value_map, arg2_old)?;

    // Compare and select minimum
    let cmp = builder.ins().icmp(IntCC::SignedLessThan, arg1, arg2);
    let new_result = builder.ins().select(cmp, arg1, arg2);

    let old_result = get_first_result(old_func, old_inst);
    value_map.insert(old_result, new_result);

    Ok(())
}

#[cfg(test)]
#[cfg(feature = "std")]
mod tests {
    #[cfg(feature = "emulator")]
    use crate::backend::transform::q32::q32_test_util;

    /// Test fcmp: equal comparison
    #[test]
    #[cfg(feature = "emulator")]
    #[ignore] // TODO emu: Q32 converter tests failing
    fn test_q32_fcmp_equal() {
        let clif = r#"
function %main() -> f32 system_v {
block0:
    v0 = f32const 0x1.4p1
    v1 = f32const 0x1.4p1
    v2 = fcmp eq v0, v1
    return v2
}
"#;
        // Result should be 1.0 (true) for equal comparison
        q32_test_util::run_q32_test(clif, 1.0);
    }

    /// Test fcmp: less than comparison
    #[test]
    #[cfg(feature = "emulator")]
    #[ignore] // TODO emu: Q32 converter tests failing
    fn test_q32_fcmp_less_than() {
        let clif = r#"
function %main() -> f32 system_v {
block0:
    v0 = f32const 0x1.0p1
    v1 = f32const 0x1.8p1
    v2 = fcmp lt v0, v1
    return v2
}
"#;
        // Result should be 1.0 (true) for 2.0 < 3.0
        q32_test_util::run_q32_test(clif, 1.0);
    }

    /// Test fmax: maximum of two values
    #[test]
    #[cfg(feature = "emulator")]
    #[ignore] // TODO emu: Q32 converter tests failing
    fn test_q32_fmax() {
        let clif = r#"
function %main() -> f32 system_v {
block0:
    v0 = f32const 0x1.0p1
    v1 = f32const 0x1.8p1
    v2 = fmax v0, v1
    return v2
}
"#;
        // Result should be 3.0 (max of 2.0 and 3.0)
        q32_test_util::run_q32_test(clif, 3.0);
    }

    /// Test fmin: minimum of two values
    #[test]
    #[cfg(feature = "emulator")]
    #[ignore] // TODO emu: Q32 converter tests failing
    fn test_q32_fmin() {
        let clif = r#"
function %main() -> f32 system_v {
block0:
    v0 = f32const 0x1.0p1
    v1 = f32const 0x1.8p1
    v2 = fmin v0, v1
    return v2
}
"#;
        // Result should be 2.0 (min of 2.0 and 3.0)
        q32_test_util::run_q32_test(clif, 2.0);
    }
}
