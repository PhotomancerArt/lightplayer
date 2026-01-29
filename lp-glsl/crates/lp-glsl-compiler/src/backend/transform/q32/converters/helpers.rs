//! Common helper functions for instruction conversion.

use crate::backend::transform::q32::types::FixedPointFormat;
use crate::error::{ErrorCode, GlslError};
use alloc::format;
use cranelift_codegen::ir::{Function, Inst, InstBuilder, InstructionData, Value};
use cranelift_frontend::FunctionBuilder;
use hashbrown::HashMap;

/// Map an old value to its new equivalent.
///
/// Resolves aliases in the old function before mapping to ensure correct value translation.
/// Returns an error if the value is not found in value_map, as Values are function-scoped
/// and we cannot use Values from the old function in the new function.
pub fn map_value(
    old_func: &Function,
    value_map: &HashMap<Value, Value>,
    old_value: Value,
) -> Result<Value, GlslError> {
    // Resolve aliases in the old function first
    // This is critical: if old_value is an alias (e.g., v10 -> v16), we need to resolve
    // it to the actual value (v16) before looking it up in the value_map
    let resolved_value = old_func.dfg.resolve_aliases(old_value);

    // Now map the resolved value
    // Values are function-scoped, so we MUST find it in value_map - we cannot use the old value
    value_map.get(&resolved_value).copied().ok_or_else(|| {
        GlslError::new(
            ErrorCode::E0301,
            format!(
                "Value {resolved_value:?} (resolved from {old_value:?}) not found in value_map. \
                 This indicates a bug in instruction copying - all values must be copied/transformed \
                 before they are used. The value may be from a block that hasn't been processed yet, \
                 or it may be a constant that needs to be pre-created."
            ),
        )
    })
}

/// Map a value through the value map (alias for consistency with existing code).
pub fn map_operand(
    old_func: &Function,
    value_map: &HashMap<Value, Value>,
    old_value: Value,
) -> Result<Value, GlslError> {
    map_value(old_func, value_map, old_value)
}

/// Extract binary operands from an instruction.
///
/// Returns an error if the instruction is not in Binary format.
pub fn extract_binary_operands(
    old_func: &Function,
    old_inst: Inst,
) -> Result<(Value, Value), GlslError> {
    let inst_data = &old_func.dfg.insts[old_inst];
    if let InstructionData::Binary { args, .. } = inst_data {
        Ok((args[0], args[1]))
    } else {
        Err(GlslError::new(
            ErrorCode::E0301,
            alloc::format!(
                "Expected binary instruction format, got: {:?} (opcode: {:?})",
                inst_data,
                old_func.dfg.insts[old_inst].opcode()
            ),
        ))
    }
}

/// Extract unary operand from an instruction.
///
/// Returns an error if the instruction is not in Unary format.
pub fn extract_unary_operand(old_func: &Function, old_inst: Inst) -> Result<Value, GlslError> {
    let inst_data = &old_func.dfg.insts[old_inst];
    if let InstructionData::Unary { arg, .. } = inst_data {
        Ok(*arg)
    } else {
        Err(GlslError::new(
            ErrorCode::E0301,
            alloc::format!(
                "Expected unary instruction format, got: {:?} (opcode: {:?})",
                inst_data,
                old_func.dfg.insts[old_inst].opcode()
            ),
        ))
    }
}

/// Get the first result value from an instruction.
pub fn get_first_result(old_func: &Function, old_inst: Inst) -> Value {
    old_func.dfg.first_result(old_inst)
}

/// Create a zero constant for the target fixed-point type.
pub fn create_zero_const(builder: &mut FunctionBuilder, format: FixedPointFormat) -> Value {
    let target_type = format.cranelift_type();
    builder.ins().iconst(target_type, 0)
}
