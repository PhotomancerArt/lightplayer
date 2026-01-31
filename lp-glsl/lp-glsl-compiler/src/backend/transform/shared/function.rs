//! Shared function transformation orchestration
//!
//! This module provides the main function transformation sequence that is shared
//! across all transforms. Transforms only need to provide instruction transformation
//! and type mapping callbacks.

use crate::backend::transform::shared::{copy_stack_slots, create_blocks, map_entry_block_params};
use crate::error::{ErrorCode, GlslError};
use alloc::vec::Vec;
use cranelift_codegen::ir::{Block, Function, Inst, Signature, StackSlot, Value};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use hashbrown::HashMap;

/// Transform a function body using the provided callbacks.
///
/// This function orchestrates the complete transformation sequence:
/// 1. Create new function with transformed signature
/// 2. Copy stack slots
/// 3. Create blocks and map entry params
/// 4. Transform/copy instructions (using callback)
/// 5. Seal blocks and finalize builder
/// 6. Copy value aliases
///
/// # Parameters
///
/// * `old_func` - The original function to transform
/// * `new_sig` - The transformed signature (already converted)
/// * `transform_inst` - Callback to transform/copy each instruction
/// * `map_param_type` - Callback to map block parameter types
///
/// # Returns
///
/// The transformed function, or an error if transformation fails.
pub fn transform_function_body(
    old_func: &Function,
    new_sig: Signature,
    transform_inst: impl Fn(
        &Function,
        Inst,
        &mut FunctionBuilder,
        &mut HashMap<Value, Value>,
        Option<&HashMap<StackSlot, StackSlot>>,
        &HashMap<Block, Block>,
    ) -> Result<(), GlslError>,
) -> Result<Function, GlslError> {
    // 1. Create new function with transformed signature
    let mut new_func = Function::with_name_signature(old_func.name.clone(), new_sig);

    // 2. Copy stack slots (with type mapping)
    let stack_slot_map = copy_stack_slots(old_func, &mut new_func)?;

    // 3. Create builder
    let mut builder_ctx = FunctionBuilderContext::new();
    let mut builder = FunctionBuilder::new(&mut new_func, &mut builder_ctx);

    // 4. Create maps
    let mut block_map = HashMap::new();
    let mut value_map = HashMap::new();

    // 5. Create blocks and map entry params
    create_blocks(old_func, &mut builder, &mut block_map, &mut value_map)?;

    // 6. Get entry block and verify params
    let entry_block = old_func
        .layout
        .entry_block()
        .ok_or_else(|| GlslError::new(ErrorCode::E0301, "Function has no entry block"))?;
    let new_entry_block = block_map[&entry_block];

    // Verify entry block params
    map_entry_block_params(
        old_func,
        entry_block,
        new_entry_block,
        &mut builder,
        &value_map,
    )?;

    // 7. Transform/copy all instructions
    for old_block in old_func.layout.blocks() {
        let insts: Vec<Inst> = old_func.layout.block_insts(old_block).collect();

        let new_block = block_map[&old_block];
        builder.switch_to_block(new_block);

        for old_inst in insts {
            transform_inst(
                old_func,
                old_inst,
                &mut builder,
                &mut value_map,
                Some(&stack_slot_map),
                &block_map,
            )?;
        }
    }

    // 8. Seal all blocks
    builder.seal_all_blocks();

    // 9. Finalize builder
    builder.finalize();

    Ok(new_func)
}
