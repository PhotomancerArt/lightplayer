//! Boolean operation conversion functions.

use crate::backend::transform::q32::converters::{
    extract_binary_operands, extract_unary_operand, get_first_result, map_operand,
};
use crate::error::GlslError;
use cranelift_codegen::ir::{Function, Inst, InstBuilder};
use cranelift_frontend::FunctionBuilder;
use hashbrown::HashMap;

/// Convert Band instruction, handling type inference from operands.
///
/// When operands are i32 (from fixed-point comparisons), use band.i32.
/// When operands are i8 (original boolean), use band.i8.
pub(crate) fn convert_band(
    old_func: &Function,
    old_inst: Inst,
    builder: &mut FunctionBuilder,
    value_map: &mut HashMap<cranelift_codegen::ir::Value, cranelift_codegen::ir::Value>,
) -> Result<(), GlslError> {
    let (arg1_old, arg2_old) = extract_binary_operands(old_func, old_inst)?;
    let arg1 = map_operand(old_func, value_map, arg1_old)?;
    let arg2 = map_operand(old_func, value_map, arg2_old)?;

    // Type is inferred from operands
    let result = builder.ins().band(arg1, arg2);

    let old_result = get_first_result(old_func, old_inst);
    value_map.insert(old_result, result);

    Ok(())
}

/// Convert Bor instruction, handling type inference from operands.
pub(crate) fn convert_bor(
    old_func: &Function,
    old_inst: Inst,
    builder: &mut FunctionBuilder,
    value_map: &mut HashMap<cranelift_codegen::ir::Value, cranelift_codegen::ir::Value>,
) -> Result<(), GlslError> {
    let (arg1_old, arg2_old) = extract_binary_operands(old_func, old_inst)?;
    let arg1 = map_operand(old_func, value_map, arg1_old)?;
    let arg2 = map_operand(old_func, value_map, arg2_old)?;

    // Type is inferred from operands
    let result = builder.ins().bor(arg1, arg2);

    let old_result = get_first_result(old_func, old_inst);
    value_map.insert(old_result, result);

    Ok(())
}

/// Convert Bxor instruction, handling type inference from operands.
pub(crate) fn convert_bxor(
    old_func: &Function,
    old_inst: Inst,
    builder: &mut FunctionBuilder,
    value_map: &mut HashMap<cranelift_codegen::ir::Value, cranelift_codegen::ir::Value>,
) -> Result<(), GlslError> {
    let (arg1_old, arg2_old) = extract_binary_operands(old_func, old_inst)?;
    let arg1 = map_operand(old_func, value_map, arg1_old)?;
    let arg2 = map_operand(old_func, value_map, arg2_old)?;

    // Type is inferred from operands
    let result = builder.ins().bxor(arg1, arg2);

    let old_result = get_first_result(old_func, old_inst);
    value_map.insert(old_result, result);

    Ok(())
}

/// Convert Bnot instruction, handling type inference from operand.
pub(crate) fn convert_bnot(
    old_func: &Function,
    old_inst: Inst,
    builder: &mut FunctionBuilder,
    value_map: &mut HashMap<cranelift_codegen::ir::Value, cranelift_codegen::ir::Value>,
) -> Result<(), GlslError> {
    let arg_old = extract_unary_operand(old_func, old_inst)?;
    let arg = map_operand(old_func, value_map, arg_old)?;

    // Type is inferred from operand
    let result = builder.ins().bnot(arg);

    let old_result = get_first_result(old_func, old_inst);
    value_map.insert(old_result, result);

    Ok(())
}
