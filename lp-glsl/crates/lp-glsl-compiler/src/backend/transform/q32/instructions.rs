//! Instruction conversion routing for fixed-point transformation.

use crate::backend::transform::q32::converters;
use crate::backend::transform::q32::types::FixedPointFormat;
use crate::backend::transform::shared::copy_instruction;
use crate::error::GlslError;
use alloc::string::String;
use cranelift_codegen::ir::{Block, FuncRef, Function, Inst, Opcode, SigRef, StackSlot, Value};
use cranelift_frontend::FunctionBuilder;
use cranelift_module::FuncId;
use hashbrown::HashMap;

/// State for function call conversion (FuncRef and SigRef mapping)
pub struct CallConversionState {
    pub ext_func_map: HashMap<FuncRef, FuncRef>,
    pub sig_map: HashMap<SigRef, SigRef>,
}

impl CallConversionState {
    pub fn new() -> Self {
        Self {
            ext_func_map: HashMap::new(),
            sig_map: HashMap::new(),
        }
    }
}

/// Convert all instructions for a function.
///
/// This is called from transform_function_body for each instruction.
pub(crate) fn convert_all_instructions(
    old_func: &Function,
    old_inst: Inst,
    builder: &mut FunctionBuilder,
    value_map: &mut HashMap<Value, Value>,
    format: FixedPointFormat,
    block_map: &HashMap<Block, Block>,
    stack_slot_map: Option<&HashMap<StackSlot, StackSlot>>,
    call_state: &mut CallConversionState,
    func_id_map: &HashMap<String, FuncId>,
    old_func_id_map: &HashMap<FuncId, String>,
    pointer_type: cranelift_codegen::ir::Type,
) -> Result<(), GlslError> {
    convert_instruction(
        old_func,
        old_inst,
        builder,
        value_map,
        format,
        block_map,
        stack_slot_map,
        call_state,
        func_id_map,
        old_func_id_map,
        pointer_type,
    )
}

/// Convert a single instruction.
fn convert_instruction(
    old_func: &Function,
    old_inst: Inst,
    builder: &mut FunctionBuilder,
    value_map: &mut HashMap<Value, Value>,
    format: FixedPointFormat,
    block_map: &HashMap<Block, Block>,
    stack_slot_map: Option<&HashMap<StackSlot, StackSlot>>,
    call_state: &mut CallConversionState,
    func_id_map: &HashMap<String, FuncId>,
    old_func_id_map: &HashMap<FuncId, String>,
    pointer_type: cranelift_codegen::ir::Type,
) -> Result<(), GlslError> {
    // Copy source location
    let srcloc = old_func.srcloc(old_inst);
    if !srcloc.is_default() {
        builder.set_srcloc(srcloc);
    }

    let opcode = old_func.dfg.insts[old_inst].opcode();

    // Route to appropriate converter
    match opcode {
        Opcode::F32const => {
            converters::constants::convert_f32const(
                old_func, old_inst, builder, value_map, format,
            )?;
        }
        Opcode::Fadd => {
            converters::arithmetic::convert_fadd(
                old_func,
                old_inst,
                builder,
                value_map,
                format,
                func_id_map,
            )?;
        }
        Opcode::Fsub => {
            converters::arithmetic::convert_fsub(
                old_func,
                old_inst,
                builder,
                value_map,
                format,
                func_id_map,
            )?;
        }
        Opcode::Fmul => {
            converters::arithmetic::convert_fmul(
                old_func,
                old_inst,
                builder,
                value_map,
                format,
                func_id_map,
            )?;
        }
        Opcode::Fdiv => {
            converters::arithmetic::convert_fdiv(
                old_func,
                old_inst,
                builder,
                value_map,
                format,
                func_id_map,
            )?;
        }
        Opcode::Fneg => {
            converters::arithmetic::convert_fneg(old_func, old_inst, builder, value_map, format)?;
        }
        Opcode::Fabs => {
            converters::arithmetic::convert_fabs(old_func, old_inst, builder, value_map, format)?;
        }
        Opcode::Call => {
            converters::calls::convert_call(
                old_func,
                old_inst,
                builder,
                value_map,
                &mut call_state.ext_func_map,
                &mut call_state.sig_map,
                format,
                func_id_map,
                old_func_id_map,
                pointer_type,
            )?;
        }
        Opcode::CallIndirect => {
            converters::calls::convert_call_indirect(
                old_func,
                old_inst,
                builder,
                value_map,
                &mut call_state.sig_map,
                format,
            )?;
        }
        Opcode::Fcmp => {
            converters::comparison::convert_fcmp(old_func, old_inst, builder, value_map, format)?;
        }
        Opcode::Fmax => {
            converters::comparison::convert_fmax(old_func, old_inst, builder, value_map, format)?;
        }
        Opcode::Fmin => {
            converters::comparison::convert_fmin(old_func, old_inst, builder, value_map, format)?;
        }
        Opcode::Load => {
            converters::memory::convert_load(old_func, old_inst, builder, value_map, format)?;
        }
        Opcode::Store => {
            converters::memory::convert_store(old_func, old_inst, builder, value_map, format)?;
        }
        Opcode::Ceil => {
            converters::math::convert_ceil(old_func, old_inst, builder, value_map, format)?;
        }
        Opcode::Floor => {
            converters::math::convert_floor(old_func, old_inst, builder, value_map, format)?;
        }
        Opcode::Trunc => {
            converters::math::convert_trunc(old_func, old_inst, builder, value_map, format)?;
        }
        Opcode::Nearest => {
            converters::math::convert_nearest(old_func, old_inst, builder, value_map, format)?;
        }
        Opcode::Sqrt => {
            converters::math::convert_sqrt(
                old_func,
                old_inst,
                builder,
                value_map,
                format,
                func_id_map,
            )?;
        }
        Opcode::FcvtFromSint => {
            converters::conversions::convert_fcvt_from_sint(
                old_func, old_inst, builder, value_map, format,
            )?;
        }
        Opcode::FcvtFromUint => {
            converters::conversions::convert_fcvt_from_uint(
                old_func, old_inst, builder, value_map, format,
            )?;
        }
        Opcode::FcvtToSint => {
            converters::conversions::convert_fcvt_to_sint(
                old_func, old_inst, builder, value_map, format,
            )?;
        }
        Opcode::FcvtToUint => {
            converters::conversions::convert_fcvt_to_uint(
                old_func, old_inst, builder, value_map, format,
            )?;
        }
        // Boolean operations: use operand types (may be i32 from fixed-point comparisons)
        Opcode::Band => {
            converters::boolean::convert_band(old_func, old_inst, builder, value_map)?;
        }
        Opcode::Bor => {
            converters::boolean::convert_bor(old_func, old_inst, builder, value_map)?;
        }
        Opcode::Bxor => {
            converters::boolean::convert_bxor(old_func, old_inst, builder, value_map)?;
        }
        Opcode::Bnot => {
            converters::boolean::convert_bnot(old_func, old_inst, builder, value_map)?;
        }
        // ... more F32 instructions as we add them ...
        _ => {
            // For non-F32 instructions, fall back to base copier
            // Type mapping: F32 → I32, others unchanged
            // Note: copy_instruction expects Option<&HashMap<String, FuncRef>> but we have
            // HashMap<FuncRef, FuncRef>. Since copy_instruction doesn't use func_ref_map
            // (it's marked unused), we pass None.
            copy_instruction(
                old_func,
                old_inst,
                builder,
                value_map,
                stack_slot_map,
                block_map,
                None, // func_ref_map not used by copy_instruction
                |ty| {
                    if ty == cranelift_codegen::ir::types::F32 {
                        cranelift_codegen::ir::types::I32
                    } else if ty == cranelift_codegen::ir::types::I8 {
                        // Keep i8 types unchanged (for boolean operations)
                        cranelift_codegen::ir::types::I8
                    } else {
                        ty
                    }
                },
            )?;
        }
    }

    Ok(())
}
