//! Type conversion instruction conversion functions.

use crate::backend::transform::q32::converters::{
    extract_unary_operand, get_first_result, map_operand,
};
use crate::backend::transform::q32::types::FixedPointFormat;
use crate::error::GlslError;
use cranelift_codegen::ir::{Function, Inst, InstBuilder, Value, types};
use cranelift_frontend::FunctionBuilder;
use hashbrown::HashMap;

/// Convert FcvtFromSint instruction.
pub(crate) fn convert_fcvt_from_sint(
    old_func: &Function,
    old_inst: Inst,
    builder: &mut FunctionBuilder,
    value_map: &mut HashMap<Value, Value>,
    format: FixedPointFormat,
) -> Result<(), GlslError> {
    // Check if result is F32
    let old_result = get_first_result(old_func, old_inst);
    if old_func.dfg.value_type(old_result) != types::F32 {
        // Not an F32 conversion, skip
        return Ok(());
    }

    let arg = extract_unary_operand(old_func, old_inst)?;

    // Map argument
    let mapped_arg = map_operand(old_func, value_map, arg)?;
    let target_type = format.cranelift_type();
    let shift_amount = format.shift_amount();

    // Convert integer to fixed-point: int << shift_amount
    // Need to sign-extend if arg is smaller than target_type
    // For fixed16x16, clamp values to range [-32768, 32767] BEFORE shifting
    // to avoid overflow. After shifting, this becomes [-32768.0, 32767.0]
    let arg_type = old_func.dfg.value_type(arg);
    let shift_const = builder.ins().iconst(target_type, shift_amount);

    // Clamp integer values before shifting to avoid overflow
    // Max int: 32767, Min int: -32768
    let max_int = builder.ins().iconst(target_type, 32767i64);
    let min_int = builder.ins().iconst(target_type, -32768i64);
    let clamped_max = builder.ins().smin(mapped_arg, max_int);
    let clamped_int = builder.ins().smax(clamped_max, min_int);

    let shifted = if arg_type.bits() < target_type.bits() {
        // Sign-extend first, then shift
        let extended = builder.ins().sextend(target_type, clamped_int);
        builder.ins().ishl(extended, shift_const)
    } else {
        // Direct shift
        builder.ins().ishl(clamped_int, shift_const)
    };

    value_map.insert(old_result, shifted);

    Ok(())
}

/// Convert FcvtFromUint instruction.
pub(crate) fn convert_fcvt_from_uint(
    old_func: &Function,
    old_inst: Inst,
    builder: &mut FunctionBuilder,
    value_map: &mut HashMap<Value, Value>,
    format: FixedPointFormat,
) -> Result<(), GlslError> {
    // Check if result is F32
    let old_result = get_first_result(old_func, old_inst);
    if old_func.dfg.value_type(old_result) != types::F32 {
        // Not an F32 conversion, skip
        return Ok(());
    }

    let arg = extract_unary_operand(old_func, old_inst)?;

    // Map argument
    let mapped_arg = map_operand(old_func, value_map, arg)?;
    let target_type = format.cranelift_type();
    let shift_amount = format.shift_amount();

    // Convert unsigned integer to fixed-point: uint << shift_amount
    // Need to zero-extend if arg is smaller than target_type
    // For same-size types (both I32), we need to handle uint values that appear negative
    // when stored as signed i32 (e.g., 4294967295u appears as -1)
    let arg_type = old_func.dfg.value_type(arg);
    let shift_const = builder.ins().iconst(target_type, shift_amount);

    let shifted = if arg_type.bits() < target_type.bits() {
        // Zero-extend first, then shift
        let extended = builder.ins().uextend(target_type, mapped_arg);
        builder.ins().ishl(extended, shift_const)
    } else if arg_type.bits() == target_type.bits() {
        // Same size (both I32): uint values stored as i32 can appear negative
        // (e.g., 4294967295u appears as -1). For fixed16x16, max representable
        // value is 32767.0, so we clamp large uint values BEFORE shifting.
        // Clamp to max uint value that fits: 32767
        let max_uint = builder.ins().iconst(target_type, 32767i64);
        // Use unsigned min to clamp (treating i32 as unsigned)
        let i64_type = types::I64;
        let extended_arg = builder.ins().uextend(i64_type, mapped_arg);
        let extended_max = builder.ins().uextend(i64_type, max_uint);
        let clamped_i64 = builder.ins().umin(extended_arg, extended_max);
        let clamped = builder.ins().ireduce(target_type, clamped_i64);

        // Now shift the clamped value
        builder.ins().ishl(clamped, shift_const)
    } else {
        // arg_type.bits() > target_type.bits(): truncate first, then shift
        let truncated = builder.ins().ireduce(target_type, mapped_arg);
        builder.ins().ishl(truncated, shift_const)
    };

    value_map.insert(old_result, shifted);

    Ok(())
}

/// Convert FcvtToSint instruction.
/// In q32 mode, floats are represented as integers shifted left by shift_amount.
/// Converting float to int means: truncate(float_value) = truncate(int_value / 2^shift) = int_value >> shift_amount
pub(crate) fn convert_fcvt_to_sint(
    old_func: &Function,
    old_inst: Inst,
    builder: &mut FunctionBuilder,
    value_map: &mut HashMap<Value, Value>,
    format: FixedPointFormat,
) -> Result<(), GlslError> {
    let old_result = get_first_result(old_func, old_inst);
    let result_ty = old_func.dfg.value_type(old_result);

    // Only handle F32 -> I32 conversions
    let arg = extract_unary_operand(old_func, old_inst)?;
    if old_func.dfg.value_type(arg) != types::F32 {
        // Not an F32 conversion, skip
        return Ok(());
    }

    // Map the fixed-point integer argument (representing a float)
    let mapped_arg = map_operand(old_func, value_map, arg)?;
    let shift_amount = format.shift_amount();
    let target_type = format.cranelift_type();

    // Convert fixed-point integer to int by arithmetic right shift
    // For truncation toward zero:
    // - Positive numbers: value >> shift (arithmetic right shift works correctly)
    // - Negative numbers: need to add bias to round toward zero instead of negative infinity
    //   Formula: (value + (1 << shift) - 1) >> shift for negative numbers
    let shift_const = builder.ins().iconst(target_type, shift_amount);
    let zero = builder.ins().iconst(target_type, 0);
    let bias_mask = builder
        .ins()
        .iconst(target_type, (1i64 << shift_amount) - 1);

    // Check if value is negative
    let is_negative = builder.ins().icmp(
        cranelift_codegen::ir::condcodes::IntCC::SignedLessThan,
        mapped_arg,
        zero,
    );

    // For negative numbers, add bias before shifting; for positive, shift directly
    let biased_arg = builder.ins().iadd(mapped_arg, bias_mask);
    let biased_value = builder.ins().select(is_negative, biased_arg, mapped_arg);
    let result = builder.ins().sshr(biased_value, shift_const);

    // If result type is smaller than target_type, truncate
    let final_result = if result_ty.bits() < target_type.bits() {
        builder.ins().ireduce(result_ty, result)
    } else if result_ty.bits() > target_type.bits() {
        builder.ins().sextend(result_ty, result)
    } else {
        result
    };

    value_map.insert(old_result, final_result);

    Ok(())
}

/// Convert FcvtToUint instruction.
/// In q32 mode, floats are represented as integers shifted left by shift_amount.
/// Converting float to uint means: truncate(float_value) = truncate(int_value / 2^shift) = int_value >> shift_amount
/// Note: GLSL spec says converting negative float to uint wraps (modulo 2^32).
/// We wrap negative values by converting to i32 first, then casting to u32 (which wraps automatically).
pub(crate) fn convert_fcvt_to_uint(
    old_func: &Function,
    old_inst: Inst,
    builder: &mut FunctionBuilder,
    value_map: &mut HashMap<Value, Value>,
    format: FixedPointFormat,
) -> Result<(), GlslError> {
    let old_result = get_first_result(old_func, old_inst);
    let result_ty = old_func.dfg.value_type(old_result);

    // Only handle F32 -> I32 conversions (uint uses I32 in Cranelift)
    let arg = extract_unary_operand(old_func, old_inst)?;
    if old_func.dfg.value_type(arg) != types::F32 {
        // Not an F32 conversion, skip
        return Ok(());
    }

    // Map the fixed-point integer argument (representing a float)
    let mapped_arg = map_operand(old_func, value_map, arg)?;
    let shift_amount = format.shift_amount();
    let target_type = format.cranelift_type();

    // Truncate toward zero: shift right, but for negative values we need to round up (toward zero)
    let shift_const = builder.ins().iconst(target_type, shift_amount);
    let zero = builder.ins().iconst(target_type, 0);
    let is_negative = builder.ins().icmp(
        cranelift_codegen::ir::condcodes::IntCC::SignedLessThan,
        mapped_arg,
        zero,
    );

    // For truncation toward zero:
    // - Positive values: shift right (rounds down toward zero) ✓
    // - Negative values: need to round up toward zero
    //   Add (1 << shift_amount) - 1 before shifting to round up
    let mask_value = (1u64 << shift_amount) - 1;
    let mask = builder.ins().iconst(target_type, mask_value as i64);
    let adjusted_negative = builder.ins().iadd(mapped_arg, mask);
    let shifted_negative = builder.ins().sshr(adjusted_negative, shift_const);
    let shifted_positive = builder.ins().sshr(mapped_arg, shift_const);

    // Select based on sign: use rounded-up value for negatives, normal shift for positives
    let result = builder
        .ins()
        .select(is_negative, shifted_negative, shifted_positive);

    // If result type is smaller than target_type, truncate
    let final_result = if result_ty.bits() < target_type.bits() {
        builder.ins().ireduce(result_ty, result)
    } else if result_ty.bits() > target_type.bits() {
        // Zero-extend for unsigned
        builder.ins().uextend(result_ty, result)
    } else {
        result
    };

    value_map.insert(old_result, final_result);

    Ok(())
}
