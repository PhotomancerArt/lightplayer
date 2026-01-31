//! Shared utility for copying instructions between functions
//!
//! This module provides a common implementation for copying instructions
//! from one function to another, mapping values through a value_map.
//! Used by all transforms.

use crate::backend::transform::shared::blocks::ensure_block_params;
use crate::error::{ErrorCode, GlslError};
use cranelift_codegen::ir::{
    Block, BlockArg, ExternalName, FuncRef, Function, Inst, InstBuilder, InstructionData,
    JumpTableData, StackSlot, Type, Value,
};
use cranelift_frontend::FunctionBuilder;
use hashbrown::HashMap;

use alloc::{format, string::String, vec::Vec};

/// Inline map_value utility
/// Resolves aliases in the old function before mapping to ensure correct value translation
///
/// Returns an error if the value is not found in value_map, as Values are function-scoped
/// and we cannot use Values from the old function in the new function.
fn map_value(
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

/// Copy an instruction from old function to new function.
///
/// Handles all instruction formats including terminators (Jump, Brif, Return, BrTable),
/// calls (Call, CallIndirect), and all other instruction types.
///
/// Also copies source location from the old instruction to the new instruction.
///
/// # Parameters
///
/// * `func_ref_map` - Optional mapping from function names to FuncRefs (currently unused,
///   kept for API compatibility with TransformContext).
/// * `map_param_type` - Callback to map block parameter types (e.g., F32 → I32 for q32 transform)
pub fn copy_instruction(
    old_func: &Function,
    old_inst: Inst,
    builder: &mut FunctionBuilder,
    value_map: &mut HashMap<Value, Value>,
    stack_slot_map: Option<&HashMap<StackSlot, StackSlot>>,
    block_map: &HashMap<Block, Block>,
    _func_ref_map: Option<&HashMap<String, FuncRef>>,
    map_param_type: impl Fn(Type) -> Type,
) -> Result<(), GlslError> {
    // Copy source location from old instruction
    let srcloc = old_func.srcloc(old_inst);
    if !srcloc.is_default() {
        builder.set_srcloc(srcloc);
    }

    let opcode = old_func.dfg.insts[old_inst].opcode();

    // Fcmp should never be copied - it must be converted to icmp for fixed-point
    if opcode == cranelift_codegen::ir::Opcode::Fcmp {
        return Err(GlslError::new(
            crate::error::ErrorCode::E0301,
            alloc::format!(
                "Fcmp instruction {old_inst:?} should be converted, not copied. This is an internal error."
            ),
        ));
    }
    let inst_data = &old_func.dfg.insts[old_inst];

    // Handle terminators first (they don't produce results)
    match inst_data {
        InstructionData::Jump { destination, .. } => {
            // Map destination block
            let old_dest_block = destination.block(&old_func.dfg.value_lists);
            let new_dest_block = block_map[&old_dest_block];

            // Ensure target block has the required parameters
            // Use a block to make the borrow scope explicit
            {
                ensure_block_params(
                    old_func,
                    old_dest_block,
                    new_dest_block,
                    builder,
                    value_map,
                    &map_param_type,
                )?;
            }

            // Map arguments
            let old_args: Vec<Value> = destination
                .args(&old_func.dfg.value_lists)
                .filter_map(|arg| arg.as_value())
                .collect();
            let new_args: Vec<BlockArg> = old_args
                .iter()
                .map(|&v| map_value(old_func, value_map, v))
                .collect::<Result<Vec<_>, _>>()?
                .into_iter()
                .map(|v| v.into())
                .collect();

            // Emit jump
            builder.ins().jump(new_dest_block, &new_args);
            return Ok(());
        }
        InstructionData::Brif {
            arg,
            blocks: [block_then_call, block_else_call],
            ..
        } => {
            // Map condition
            let condition = map_value(old_func, value_map, *arg)?;

            // Extract blocks from BlockCalls
            let old_then_block = block_then_call.block(&old_func.dfg.value_lists);
            let old_else_block = block_else_call.block(&old_func.dfg.value_lists);

            // Map destination blocks
            let new_then_block = block_map[&old_then_block];
            let new_else_block = block_map[&old_else_block];

            // Ensure target blocks have the required parameters
            // Use blocks to make the borrow scopes explicit
            {
                ensure_block_params(
                    old_func,
                    old_then_block,
                    new_then_block,
                    builder,
                    value_map,
                    &map_param_type,
                )?;
            }
            {
                ensure_block_params(
                    old_func,
                    old_else_block,
                    new_else_block,
                    builder,
                    value_map,
                    &map_param_type,
                )?;
            }

            // Map block arguments
            let old_then_args: Vec<Value> = block_then_call
                .args(&old_func.dfg.value_lists)
                .filter_map(|arg| arg.as_value())
                .collect();
            let old_else_args: Vec<Value> = block_else_call
                .args(&old_func.dfg.value_lists)
                .filter_map(|arg| arg.as_value())
                .collect();

            let new_then_args: Vec<BlockArg> = old_then_args
                .iter()
                .map(|&v| map_value(old_func, value_map, v))
                .collect::<Result<Vec<_>, _>>()?
                .into_iter()
                .map(|v| v.into())
                .collect();
            let new_else_args: Vec<BlockArg> = old_else_args
                .iter()
                .map(|&v| map_value(old_func, value_map, v))
                .collect::<Result<Vec<_>, _>>()?
                .into_iter()
                .map(|v| v.into())
                .collect();

            // Emit brif
            builder.ins().brif(
                condition,
                new_then_block,
                &new_then_args,
                new_else_block,
                &new_else_args,
            );
            return Ok(());
        }
        InstructionData::BranchTable { arg, table, .. } => {
            // Map condition
            let condition = map_value(old_func, value_map, *arg)?;

            // Get old jump table
            let old_table = &old_func.dfg.jump_tables[*table];

            // Map default destination (first element in jump table)
            let old_default_block_call = old_table.default_block();
            let old_default_block = old_default_block_call.block(&old_func.dfg.value_lists);
            let new_default_block = block_map[&old_default_block];

            // Ensure default block has the required parameters
            {
                ensure_block_params(
                    old_func,
                    old_default_block,
                    new_default_block,
                    builder,
                    value_map,
                    &map_param_type,
                )?;
            }

            // Map default block arguments
            let old_default_args: Vec<Value> = old_default_block_call
                .args(&old_func.dfg.value_lists)
                .filter_map(|arg| arg.as_value())
                .collect();
            let new_default_args: Vec<BlockArg> = old_default_args
                .iter()
                .map(|&v| map_value(old_func, value_map, v))
                .collect::<Result<Vec<_>, _>>()?
                .into_iter()
                .map(|v| v.into())
                .collect();
            let new_default_block_call = builder
                .func
                .dfg
                .block_call(new_default_block, &new_default_args);

            // Map table destinations
            let mut new_table_blocks = Vec::new();
            for old_block_call in old_table.as_slice() {
                let old_block = old_block_call.block(&old_func.dfg.value_lists);
                let new_block = block_map[&old_block];

                // Ensure target block has the required parameters
                {
                    ensure_block_params(
                        old_func,
                        old_block,
                        new_block,
                        builder,
                        value_map,
                        &map_param_type,
                    )?;
                }

                // Map block arguments
                let old_args: Vec<Value> = old_block_call
                    .args(&old_func.dfg.value_lists)
                    .filter_map(|arg| arg.as_value())
                    .collect();
                let new_args: Vec<BlockArg> = old_args
                    .iter()
                    .map(|&v| map_value(old_func, value_map, v))
                    .collect::<Result<Vec<_>, _>>()?
                    .into_iter()
                    .map(|v| v.into())
                    .collect();
                let new_block_call = builder.func.dfg.block_call(new_block, &new_args);
                new_table_blocks.push(new_block_call);
            }

            // Create new jump table
            let new_table = builder.create_jump_table(JumpTableData::new(
                new_default_block_call,
                &new_table_blocks,
            ));

            // Emit br_table
            builder.ins().br_table(condition, new_table);
            return Ok(());
        }
        InstructionData::CondTrap { code, arg, .. } => {
            // Trap instructions (trapnz, trapz): map the condition value and emit trap
            let condition = map_value(old_func, value_map, *arg)?;
            if opcode == cranelift_codegen::ir::Opcode::Trapnz {
                builder.ins().trapnz(condition, *code);
            } else if opcode == cranelift_codegen::ir::Opcode::Trapz {
                builder.ins().trapz(condition, *code);
            } else {
                panic!(
                    "CondTrap instruction with unexpected opcode {opcode:?} in copy_instruction. This is an internal error - CondTrap should only be used with Trapnz or Trapz opcodes."
                );
            }
            return Ok(());
        }
        InstructionData::Trap { code, .. } => {
            // Unconditional trap: emit trap directly
            builder.ins().trap(*code);
            return Ok(());
        }
        InstructionData::MultiAry { opcode, args, .. } => {
            // Check if this is a Return instruction
            if opcode.is_return() {
                // Map return arguments
                let old_args = args.as_slice(&old_func.dfg.value_lists);
                let new_args: Vec<Value> = old_args
                    .iter()
                    .map(|&v| map_value(old_func, value_map, v))
                    .collect::<Result<Vec<_>, _>>()?;

                // Emit return
                builder.ins().return_(&new_args);
                return Ok(());
            }
            // Fall through for other MultiAry instructions
        }
        _ => {}
    }

    // Handle Call and CallIndirect
    match inst_data {
        InstructionData::Call { func_ref, args, .. } => {
            let old_args = args.as_slice(&old_func.dfg.value_lists);
            let new_args: Vec<Value> = old_args
                .iter()
                .map(|&v| map_value(old_func, value_map, v))
                .collect::<Result<Vec<_>, _>>()?;

            // Map FuncRef: import the external function into the builder's function context
            // Similar to q32 transform: import signature first, then handle external names
            let old_ext_func = &old_func.dfg.ext_funcs[*func_ref];
            let old_sig_ref = old_ext_func.signature;

            // Import the signature into the new function's context
            let old_sig = &old_func.dfg.signatures[old_sig_ref];
            let new_sig_ref = builder.func.import_signature(old_sig.clone());

            // Create new ExtFuncData with proper external name handling
            // For User external names, we need to declare the imported user function first
            use cranelift_codegen::ir::ExtFuncData;
            let new_name = match &old_ext_func.name {
                ExternalName::User(old_user_ref) => {
                    // Get the user name from the old function
                    let user_name = old_func
                        .params
                        .user_named_funcs()
                        .get(*old_user_ref)
                        .ok_or_else(|| {
                            GlslError::new(
                                ErrorCode::E0301,
                                format!(
                                    "UserExternalNameRef {old_user_ref} not found in function's user_named_funcs"
                                ),
                            )
                        })?;
                    // Declare the imported user function in the new function
                    // Note: FuncId remapping for transforms is handled at the transform level
                    // (e.g., in identity/transform.rs and q32/converters/calls.rs)
                    let new_user_ref = builder
                        .func
                        .declare_imported_user_function(user_name.clone());
                    ExternalName::User(new_user_ref)
                }
                _ => {
                    // For TestCase, LibCall, KnownSymbol - can clone directly
                    old_ext_func.name.clone()
                }
            };

            let new_ext_func = ExtFuncData {
                name: new_name,
                signature: new_sig_ref,
                colocated: old_ext_func.colocated,
            };

            // Import the external function into the builder's function context
            // This creates a new FuncRef scoped to the builder's function
            let new_func_ref = builder.func.import_function(new_ext_func);

            // Emit call with mapped FuncRef
            let call_inst = builder.ins().call(new_func_ref, &new_args);

            // Map results
            let old_results: Vec<Value> = old_func.dfg.inst_results(old_inst).to_vec();
            let new_results = builder.inst_results(call_inst);
            if old_results.len() != new_results.len() {
                return Err(GlslError::new(
                    ErrorCode::E0301,
                    format!(
                        "Call instruction result count mismatch: old={}, new={}",
                        old_results.len(),
                        new_results.len()
                    ),
                ));
            }
            for (old_result, new_result) in old_results.iter().zip(new_results.iter()) {
                value_map.insert(*old_result, *new_result);
            }
            return Ok(());
        }
        InstructionData::CallIndirect { sig_ref, args, .. } => {
            // Import the signature into the new function's context (SigRefs are scoped to functions)
            let old_sig = &old_func.dfg.signatures[*sig_ref];
            let new_sig_ref = builder.func.import_signature(old_sig.clone());

            let old_args = args.as_slice(&old_func.dfg.value_lists);
            let func_addr = map_value(old_func, value_map, old_args[0])?;
            let call_args: Vec<Value> = old_args[1..]
                .iter()
                .map(|&v| map_value(old_func, value_map, v))
                .collect::<Result<Vec<_>, _>>()?;

            // Emit indirect call with imported signature reference
            let call_inst = builder
                .ins()
                .call_indirect(new_sig_ref, func_addr, &call_args);

            // Map results
            let old_results: Vec<Value> = old_func.dfg.inst_results(old_inst).to_vec();
            let new_results = builder.inst_results(call_inst);
            if old_results.len() != new_results.len() {
                return Err(GlslError::new(
                    ErrorCode::E0301,
                    format!(
                        "CallIndirect instruction result count mismatch: old={}, new={}",
                        old_results.len(),
                        new_results.len()
                    ),
                ));
            }
            for (old_result, new_result) in old_results.iter().zip(new_results.iter()) {
                value_map.insert(*old_result, *new_result);
            }
            return Ok(());
        }
        _ => {}
    }

    // All instructions that don't produce results (terminators, traps, stores, etc.) should have
    // been handled above and returned early. If we reach here with no results, it's an error.
    let old_results: Vec<Value> = old_func.dfg.inst_results(old_inst).to_vec();
    if old_results.is_empty() {
        panic!(
            "Instruction {opcode:?} with format {inst_data:?} has no results but was not handled in copy_instruction. This is an internal error - all side-effect-only instructions must be explicitly handled."
        );
    }

    // For instructions with results, we need to reconstruct them
    // Determine the controlling type (apply type mapping if provided)
    // Special case: Fcmp should use operand type (float), not result type (i8)
    let ctrl_type = if opcode == cranelift_codegen::ir::Opcode::Fcmp {
        // Fcmp has float operands, so use the first operand's type
        let first_arg = old_func.dfg.inst_args(old_inst)[0];
        let mapped_first_arg = map_value(old_func, value_map, first_arg)?;
        let operand_type = builder.func.dfg.value_type(mapped_first_arg);
        map_param_type(operand_type)
    } else if opcode.constraints().requires_typevar_operand() {
        // Get type from first operand
        let first_arg = old_func.dfg.inst_args(old_inst)[0];
        let mapped_first_arg = map_value(old_func, value_map, first_arg)?;
        let operand_type = builder.func.dfg.value_type(mapped_first_arg);
        map_param_type(operand_type)
    } else {
        // Get type from first result and apply type mapping
        let old_result_type = old_func.dfg.value_type(old_results[0]);
        map_param_type(old_result_type)
    };

    // Get old arguments using DFG
    let old_args = old_func.dfg.inst_args(old_inst);
    let mapped_args: Vec<Value> = old_args
        .iter()
        .map(|&v| map_value(old_func, value_map, v))
        .collect::<Result<Vec<_>, _>>()?;

    // Reconstruct instruction data with mapped operands
    let new_inst_data = match inst_data {
        InstructionData::Unary { .. } => InstructionData::Unary {
            opcode,
            arg: mapped_args[0],
        },
        InstructionData::UnaryImm { imm, .. } => InstructionData::UnaryImm { opcode, imm: *imm },
        InstructionData::Binary { .. } => {
            if mapped_args.len() != 2 {
                return Err(GlslError::new(
                    ErrorCode::E0301,
                    format!(
                        "Binary instruction requires 2 arguments, got {}",
                        mapped_args.len()
                    ),
                ));
            }
            InstructionData::Binary {
                opcode,
                args: [mapped_args[0], mapped_args[1]],
            }
        }
        InstructionData::Ternary { .. } => {
            if mapped_args.len() != 3 {
                return Err(GlslError::new(
                    ErrorCode::E0301,
                    format!(
                        "Ternary instruction requires 3 arguments, got {}",
                        mapped_args.len()
                    ),
                ));
            }
            InstructionData::Ternary {
                opcode,
                args: [mapped_args[0], mapped_args[1], mapped_args[2]],
            }
        }
        InstructionData::NullAry { .. } => InstructionData::NullAry { opcode },
        InstructionData::IntCompare { cond, .. } => {
            if mapped_args.len() != 2 {
                return Err(GlslError::new(
                    ErrorCode::E0301,
                    format!(
                        "IntCompare instruction requires 2 arguments, got {}",
                        mapped_args.len()
                    ),
                ));
            }
            InstructionData::IntCompare {
                opcode,
                cond: *cond,
                args: [mapped_args[0], mapped_args[1]],
            }
        }
        InstructionData::FloatCompare { cond, .. } => {
            if mapped_args.len() != 2 {
                return Err(GlslError::new(
                    ErrorCode::E0301,
                    format!(
                        "FloatCompare instruction requires 2 arguments, got {}",
                        mapped_args.len()
                    ),
                ));
            }
            InstructionData::FloatCompare {
                opcode,
                cond: *cond,
                args: [mapped_args[0], mapped_args[1]],
            }
        }
        InstructionData::UnaryIeee32 { imm, .. } => {
            InstructionData::UnaryIeee32 { opcode, imm: *imm }
        }
        InstructionData::UnaryIeee64 { imm, .. } => {
            InstructionData::UnaryIeee64 { opcode, imm: *imm }
        }
        InstructionData::UnaryConst {
            constant_handle, ..
        } => InstructionData::UnaryConst {
            opcode,
            constant_handle: *constant_handle,
        },
        InstructionData::Load {
            flags,
            offset,
            arg: _,
            ..
        } => InstructionData::Load {
            opcode,
            flags: *flags,
            offset: *offset,
            arg: mapped_args[0],
        },
        InstructionData::Store {
            flags,
            offset,
            args: _,
            ..
        } => {
            // Store is handled separately - it has no results
            // We need to construct the instruction directly using DFG
            let store_inst_data = InstructionData::Store {
                opcode,
                flags: *flags,
                offset: *offset,
                args: [mapped_args[0], mapped_args[1]],
            };
            builder.ensure_inserted_block();
            let current_block = builder
                .current_block()
                .expect("Builder must have a current block");
            let new_inst = builder.func.dfg.make_inst(store_inst_data);
            builder.func.layout.append_inst(new_inst, current_block);
            return Ok(());
        }
        InstructionData::StackLoad {
            stack_slot, offset, ..
        } => {
            // StackLoad has stack_slot and offset
            // This format is also used by StackAddr instruction
            // Remap stack slot if mapping provided
            let new_stack_slot = if let Some(m) = stack_slot_map {
                *m.get(stack_slot).ok_or_else(|| {
                    GlslError::new(
                        ErrorCode::E0301,
                        format!(
                            "Stack slot {stack_slot:?} not found in stack_slot_map when copying instruction {opcode:?}. This indicates a bug in function copying - all stack slots must be copied before copying instructions."
                        ),
                    )
                })?
            } else {
                *stack_slot
            };
            // Both StackLoad and StackAddr use the same format
            InstructionData::StackLoad {
                opcode,
                stack_slot: new_stack_slot,
                offset: *offset,
            }
        }
        InstructionData::StackStore {
            stack_slot,
            offset,
            arg: _,
            ..
        } => {
            // Stack store is handled separately - it has no results
            // StackStore has stack_slot, offset, and arg (address)
            // Value comes from inst_args[0]
            // Remap stack slot if mapping provided
            let new_stack_slot = if let Some(m) = stack_slot_map {
                *m.get(stack_slot).ok_or_else(|| {
                    GlslError::new(
                        ErrorCode::E0301,
                        format!(
                            "Stack slot {stack_slot:?} not found in stack_slot_map when copying instruction {opcode:?}. This indicates a bug in function copying - all stack slots must be copied before copying instructions."
                        ),
                    )
                })?
            } else {
                *stack_slot
            };
            let stack_store_inst_data = InstructionData::StackStore {
                opcode,
                stack_slot: new_stack_slot,
                offset: *offset,
                arg: mapped_args[1], // address is the second argument (value is first)
            };
            builder.ensure_inserted_block();
            let current_block = builder
                .current_block()
                .expect("Builder must have a current block");
            let new_inst = builder.func.dfg.make_inst(stack_store_inst_data);
            // StackStore needs value as the first argument (address is in arg field)
            // The value argument needs to be added to the instruction
            // For now, we'll construct it with the value in the args
            builder.func.layout.append_inst(new_inst, current_block);
            return Ok(());
        }
        InstructionData::MultiAry { opcode, .. } => {
            // MultiAry instructions other than Return are rare and complex to handle generically.
            // Return is handled separately above. For other MultiAry instructions, we need
            // to handle them case-by-case or use a different approach.
            // For now, panic with a helpful message - this should be extended as needed.
            panic!(
                "MultiAry instruction {opcode:?} not yet supported in copy_instruction (Return is handled separately). This is an internal error - please add support for this instruction format."
            );
        }
        _ => {
            // For other instruction formats, panic (internal error - should never happen)
            panic!(
                "Instruction {opcode:?} with format {inst_data:?} not yet supported in copy_instruction. This is an internal error - all instruction formats should be handled. inst_data: {inst_data:?}"
            );
        }
    };

    // Insert instruction using DFG and layout directly
    // We need to ensure the block is inserted in the layout first
    builder.ensure_inserted_block();
    let current_block = builder
        .current_block()
        .expect("Builder must have a current block");

    // Create instruction
    let new_inst = builder.func.dfg.make_inst(new_inst_data);
    builder.func.dfg.make_inst_results(new_inst, ctrl_type);
    builder.func.layout.append_inst(new_inst, current_block);

    // Map results
    let new_results = builder.func.dfg.inst_results(new_inst);
    if old_results.len() != new_results.len() {
        return Err(GlslError::new(
            ErrorCode::E0301,
            format!(
                "Instruction {:?} result count mismatch: old={}, new={}",
                opcode,
                old_results.len(),
                new_results.len()
            ),
        ));
    }
    for (old_result, new_result) in old_results.iter().zip(new_results.iter()) {
        value_map.insert(*old_result, *new_result);
    }

    Ok(())
}
