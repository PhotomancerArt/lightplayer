//! Shared utilities for function transformation
//!
//! This module provides common utilities for copying CLIF function components
//! between functions. These utilities are used by all transforms.

pub mod blocks;
pub mod function;
pub mod instruction_copy;
pub mod stack_slots;

#[cfg(test)]
pub mod transform_test_util;

// Re-export all utilities for easy importing
pub use blocks::{create_blocks, ensure_block_params, map_entry_block_params};
pub use function::transform_function_body;
pub use instruction_copy::copy_instruction;
pub use stack_slots::copy_stack_slots;
