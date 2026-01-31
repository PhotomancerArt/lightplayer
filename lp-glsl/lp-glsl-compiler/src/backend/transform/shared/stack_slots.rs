//! Stack slot copying utilities

use crate::error::{ErrorCode, GlslError};
use cranelift_codegen::ir::{Function, StackSlot};
use hashbrown::HashMap;

/// Copy all stack slots from old function to new function.
///
/// Returns a mapping from old stack slot IDs to new stack slot IDs.
/// This must be called before creating the FunctionBuilder, as it needs direct access to new_func.
pub fn copy_stack_slots(
    old_func: &Function,
    new_func: &mut Function,
) -> Result<HashMap<StackSlot, StackSlot>, GlslError> {
    let mut stack_slot_map = HashMap::new();

    // Reserve space for efficiency
    new_func
        .sized_stack_slots
        .reserve(old_func.sized_stack_slots.len());

    // Copy all stack slots and build the mapping
    for (old_slot_idx, old_slot_data) in old_func.sized_stack_slots.iter() {
        // Use the actual StackSlot returned by push() instead of calculating it
        // PrimaryMap.push() returns the entity ID assigned to the new entry
        let new_slot_idx = new_func.sized_stack_slots.push(old_slot_data.clone());
        stack_slot_map.insert(old_slot_idx, new_slot_idx);

        // Verify the slot was actually added
        if !new_func.sized_stack_slots.is_valid(new_slot_idx) {
            return Err(GlslError::new(
                ErrorCode::E0301,
                alloc::format!(
                    "Failed to create stack slot {new_slot_idx:?} in new function (copied from {old_slot_idx:?})"
                ),
            ));
        }
    }

    Ok(stack_slot_map)
}
