//! Block creation and parameter management utilities

use crate::error::{ErrorCode, GlslError};
use cranelift_codegen::ir::{Block, Function, Value};
use cranelift_frontend::FunctionBuilder;
use hashbrown::HashMap;

use alloc::{format, vec::Vec};

/// Create all blocks in the new function, preserving numeric order (entity IDs) and layout order.
///
/// This function:
/// 1. Creates blocks in numeric order (by entity ID) to preserve entity IDs
/// 2. Inserts blocks into layout in layout order to preserve visual appearance
/// 3. Sets up entry block parameters (function parameters)
/// 4. Maps entry block parameters in value_map
///
/// Returns the block_map and updates value_map with entry block parameter mappings.
pub fn create_blocks(
    old_func: &Function,
    builder: &mut FunctionBuilder,
    block_map: &mut HashMap<Block, Block>,
    value_map: &mut HashMap<Value, Value>,
) -> Result<(), GlslError> {
    // Collect blocks in numeric order (by entity ID) and layout order separately
    // We need to create blocks in numeric order to preserve entity IDs, but insert them
    // into the layout in layout order to preserve the appearance order
    let old_blocks_numeric: Vec<Block> = old_func.dfg.blocks.iter().collect();
    let old_blocks_layout: Vec<Block> = old_func.layout.blocks().collect();
    let old_entry_block = old_func.layout.entry_block();

    // Step 1: Create blocks in NUMERIC order (by entity ID) so entity IDs are preserved
    // This ensures block0 gets ID 0, block1 gets ID 1, etc., matching the original
    for old_block in &old_blocks_numeric {
        let new_block = builder.create_block();
        block_map.insert(*old_block, new_block);
    }

    // Step 2: Insert blocks into layout in LAYOUT order (the order they appear in the original)
    // This preserves the visual order in the CLIF output
    let mut previous_new_block: Option<Block> = None;
    for old_block in &old_blocks_layout {
        let new_block = block_map[old_block];

        // Insert block into layout in the same order as the original function
        // Use insert_block_after to maintain order, or append_block for the first block
        if let Some(prev) = previous_new_block {
            builder.func.layout.insert_block_after(new_block, prev);
        } else {
            // First block in layout - append it (this will be the entry block)
            builder.func.layout.append_block(new_block);
        }
        previous_new_block = Some(new_block);
    }

    // Handle entry block specially - use function parameters
    if let Some(old_entry) = old_entry_block {
        let new_entry = block_map[&old_entry];

        // For entry block, use function parameters (this creates params matching the signature)
        builder.append_block_params_for_function_params(new_entry);

        // Map old entry block params to new entry block params
        let old_params = old_func.dfg.block_params(old_entry);
        let new_params = builder.block_params(new_entry);

        // Verify counts match
        if old_params.len() != new_params.len() {
            return Err(GlslError::new(
                ErrorCode::E0301,
                format!(
                    "Entry block parameter count mismatch: old={}, new={}",
                    old_params.len(),
                    new_params.len()
                ),
            ));
        }

        // Map old params to new params
        for (old_param, new_param) in old_params.iter().zip(new_params.iter()) {
            value_map.insert(*old_param, *new_param);
        }
    }

    // Don't create block parameters for non-entry blocks here
    // They will be created on-demand when we encounter jumps/brifs that target them
    // This handles the case where parameters are added dynamically via append_block_param

    Ok(())
}

/// Map function parameters (entry block params) from old to new function.
///
/// This verifies that the entry block parameters match the function signature
/// and that they're correctly mapped in value_map.
///
/// For no-op copies, this just verifies correctness.
/// For transforms, this can also verify type conversions.
pub fn map_entry_block_params(
    old_func: &Function,
    entry_block: Block,
    new_entry_block: Block,
    builder: &mut FunctionBuilder,
    value_map: &HashMap<Value, Value>,
) -> Result<(), GlslError> {
    let old_params = old_func.dfg.block_params(entry_block);
    let new_params = builder.block_params(new_entry_block);

    // Verify counts match
    if old_params.len() != new_params.len() {
        return Err(GlslError::new(
            ErrorCode::E0301,
            format!(
                "Function parameter count mismatch: old={}, new={}",
                old_params.len(),
                new_params.len()
            ),
        ));
    }

    // Verify mapping exists for all parameters
    for (old_param, new_param) in old_params.iter().zip(new_params.iter()) {
        if value_map.get(old_param) != Some(new_param) {
            return Err(GlslError::new(
                ErrorCode::E0301,
                format!(
                    "Function parameter not mapped: old_param={old_param:?}, expected new_param={new_param:?}"
                ),
            ));
        }
    }

    Ok(())
}

/// Ensure block parameters exist for a target block based on what the old block has.
///
/// This creates parameters on-demand when they're needed for jumps/brifs.
///
/// The key insight: we check what parameters the old block actually has at copy time,
/// not just what the current instruction passes. This handles the case where parameters
/// are added dynamically via append_block_param.
///
/// For no-op copies, param_type_fn should return the same type.
/// For transforms, param_type_fn can convert types (e.g., F32 to fixed-point).
pub fn ensure_block_params(
    old_func: &Function,
    old_block: Block,
    new_block: Block,
    builder: &mut FunctionBuilder,
    value_map: &mut HashMap<Value, Value>,
    param_type_fn: impl Fn(cranelift_codegen::ir::Type) -> cranelift_codegen::ir::Type,
) -> Result<(), GlslError> {
    // Get current parameter count in new block
    let current_param_count = builder.func.dfg.num_block_params(new_block);

    // Get the old block's actual parameters (what it has at copy time)
    // This is the source of truth - if the old block has N parameters, the new block should too
    let old_params = old_func.dfg.block_params(old_block);
    let expected_param_count = old_params.len();

    // If we need more parameters, add them based on the old block's parameters
    if expected_param_count > current_param_count {
        // Ensure the block is inserted in the layout (required for append_block_param)
        // Note: blocks should already be inserted by create_blocks, but we ensure it here for safety
        if !builder.func.layout.is_block_inserted(new_block) {
            builder.func.layout.append_block(new_block);
        }

        // Determine types for new parameters based on the old block's parameters
        for &old_param in old_params.iter().skip(current_param_count) {
            let old_type = old_func.dfg.value_type(old_param);
            let new_type = param_type_fn(old_type);
            builder.append_block_param(new_block, new_type);
        }

        // Map old params to new params (only the newly added ones)
        let new_params = builder.block_params(new_block);
        for i in current_param_count..expected_param_count {
            if i < old_params.len() && i < new_params.len() {
                value_map.insert(old_params[i], new_params[i]);
            }
        }
    }

    Ok(())
}
