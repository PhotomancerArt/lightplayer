//! Memory operation conversion functions.

use crate::backend::transform::q32::converters::map_value;
use crate::backend::transform::q32::types::FixedPointFormat;
use crate::error::{ErrorCode, GlslError};
use cranelift_codegen::ir::{Function, Inst, InstBuilder, InstructionData, Value, types};
use cranelift_frontend::FunctionBuilder;
use hashbrown::HashMap;

/// Convert Load instruction (if it loads F32).
pub(crate) fn convert_load(
    old_func: &Function,
    old_inst: Inst,
    builder: &mut FunctionBuilder,
    value_map: &mut HashMap<Value, Value>,
    format: FixedPointFormat,
) -> Result<(), GlslError> {
    let inst_data = &old_func.dfg.insts[old_inst];

    // Check if this is an F32 load
    let old_result = old_func.dfg.first_result(old_inst);
    let old_result_type = old_func.dfg.value_type(old_result);

    if let InstructionData::Load {
        opcode: _,
        flags,
        arg,
        offset,
    } = inst_data
    {
        // Map address
        let address = map_value(old_func, value_map, *arg)?;

        if old_result_type == types::F32 {
            // F32 load: convert to fixed-point type
            let target_type = format.cranelift_type();
            let new_result = builder.ins().load(target_type, *flags, address, *offset);
            value_map.insert(old_result, new_result);
        } else {
            // Non-F32 load: copy as-is (preserve original type)
            let new_result = builder
                .ins()
                .load(old_result_type, *flags, address, *offset);
            value_map.insert(old_result, new_result);
        }
    } else {
        return Err(GlslError::new(
            ErrorCode::E0301,
            alloc::format!("Load instruction has unexpected format: {inst_data:?}"),
        ));
    }

    Ok(())
}

/// Convert Store instruction (if it stores F32).
pub(crate) fn convert_store(
    old_func: &Function,
    old_inst: Inst,
    builder: &mut FunctionBuilder,
    value_map: &mut HashMap<Value, Value>,
    format: FixedPointFormat,
) -> Result<(), GlslError> {
    let inst_data = &old_func.dfg.insts[old_inst];

    if let InstructionData::Store {
        opcode: _,
        flags,
        args,
        offset,
    } = inst_data
    {
        // Store has args: [value, address]
        let value = args[0];
        let address = args[1];

        // Check if value being stored is F32
        let value_type = old_func.dfg.value_type(value);

        if value_type == types::F32 {
            // Map value (should already be converted)
            let mapped_value = map_value(old_func, value_map, value)?;
            let target_type = format.cranelift_type();

            // Verify mapped value has correct type
            let mapped_type = builder.func.dfg.value_type(mapped_value);
            if mapped_type != target_type {
                return Err(GlslError::new(
                    ErrorCode::E0301,
                    alloc::format!(
                        "Store value type mismatch: expected {target_type:?}, got {mapped_type:?}"
                    ),
                ));
            }

            // Map address
            let mapped_address = map_value(old_func, value_map, address)?;

            // Emit store with new type
            builder
                .ins()
                .store(*flags, mapped_value, mapped_address, *offset);
        } else {
            // Not an F32 store, copy as-is (will be handled by copy_instruction fallback)
            // But we still need to map the address in case it's F32-derived
            let mapped_address = map_value(old_func, value_map, address)?;
            let mapped_value = map_value(old_func, value_map, value)?;
            builder
                .ins()
                .store(*flags, mapped_value, mapped_address, *offset);
        }
    } else {
        return Err(GlslError::new(
            ErrorCode::E0301,
            alloc::format!("Store instruction has unexpected format: {inst_data:?}"),
        ));
    }

    Ok(())
}
