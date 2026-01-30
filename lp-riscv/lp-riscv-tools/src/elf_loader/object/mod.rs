//! Object file loading for RISC-V emulator.
//!
//! This module provides utilities to load relocatable object files into the emulator
//! after a base executable has been loaded.

#![cfg(feature = "std")]

mod layout;
mod relocations;
mod sections;
mod symbols;

#[cfg(test)]
mod tests;

// Re-export public types and functions
pub use layout::calculate_object_layout;
pub use relocations::apply_object_relocations;
pub use sections::load_object_sections;
pub use symbols::{build_object_symbol_map, merge_symbol_maps};

extern crate alloc;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use hashbrown::HashMap;

use super::parse;
use crate::elf_loader::memory::RAM_START;

/// Information about a loaded object file.
pub struct ObjectLoadInfo {
    /// Address of '_init' function if found
    pub init_address: Option<u32>,
    /// Object file's symbol map (with adjusted addresses)
    pub symbol_map: HashMap<String, u32>,
    /// Start address where .text section was placed
    pub text_start: u32,
    /// Start offset where .data section was placed (relative to RAM_START)
    pub data_start: u32,
}

/// Load a relocatable object file into the emulator after a base executable has been loaded.
///
/// This function:
/// - Parses the object file
/// - Calculates where to place sections (after base executable)
/// - Loads sections into the code/ram buffers
/// - Builds symbol map with adjusted addresses
/// - Merges symbol map with base executable's symbol map
/// - Applies relocations using the merged symbol map
/// - Updates the caller's symbol map with merged result
///
/// # Arguments
///
/// * `obj_file_bytes` - The object file bytes to load
/// * `code` - Mutable reference to code buffer (will be extended if needed)
/// * `ram` - Mutable reference to RAM buffer (will be extended if needed)
/// * `symbol_map` - Mutable reference to base executable's symbol map (will be updated with merged map)
///
/// # Returns
///
/// Information about the loaded object file, or an error if loading fails.
pub fn load_object_file(
    obj_file_bytes: &[u8],
    code: &mut Vec<u8>,
    ram: &mut Vec<u8>,
    symbol_map: &mut HashMap<String, u32>,
) -> Result<ObjectLoadInfo, String> {
    debug!("=== Loading object file ===");

    // Step 1: Parse object file
    let obj = parse::parse_elf(obj_file_bytes)?;

    // Step 2: Validate object file
    parse::validate_elf(&obj)?;

    // Step 3: Get base executable end addresses from buffer sizes
    // The buffers contain the base executable, so their current sizes indicate where it ends
    let base_code_end = code.len() as u32;
    let base_ram_end = ram.len() as u32;

    debug!(
        "Base executable: code_end=0x{:x}, ram_end=0x{:x}",
        base_code_end, base_ram_end
    );

    // Step 4: Calculate layout for object file sections
    let layout = calculate_object_layout(&obj, base_code_end, base_ram_end)?;

    // Step 5: Load object file sections into buffers
    let section_placement = load_object_sections(&obj, code, ram, &layout)?;

    // Step 6: Build object file's symbol map with adjusted addresses
    let obj_symbol_map =
        build_object_symbol_map(&obj, layout.text_placement, layout.data_placement);

    // Step 7: Merge symbol maps (base takes precedence)
    // Fail fast if there are symbol conflicts
    let merged_symbol_map = merge_symbol_maps(symbol_map, &obj_symbol_map)
        .map_err(|e| format!("Symbol merge failed: {e}"))?;

    // Step 8: Apply relocations using merged symbol map
    apply_object_relocations(&obj, code, ram, &merged_symbol_map, &section_placement)?;

    // Step 9: Update caller's symbol map with merged result
    *symbol_map = merged_symbol_map.clone();

    // Step 10: Find '_init' symbol if present in object file and update __USER_MAIN_PTR
    // Use object file's _init (not base executable's _init) for __USER_MAIN_PTR
    let init_address = obj_symbol_map.get("_init").copied();

    if let Some(init_addr) = init_address {
        // Update __USER_MAIN_PTR in both RAM and ROM LMA
        // RAM update: needed because init code may have already run when base executable was loaded
        // LMA update: needed for future init runs
        if let Some(&user_init_ptr_vma) = merged_symbol_map.get("__USER_MAIN_PTR") {
            if user_init_ptr_vma >= RAM_START {
                let ram_offset = (user_init_ptr_vma - RAM_START) as usize;

                // Always update RAM directly (init code may have already run)
                if ram_offset + 4 <= ram.len() {
                    ram[ram_offset..ram_offset + 4].copy_from_slice(&init_addr.to_le_bytes());
                }

                // Also update LMA if __data_source_start is available (for future init runs)
                // Calculate offset within .data section, not from RAM_START
                if let Some(&data_source_start) = merged_symbol_map.get("__data_source_start") {
                    // Find __data_target_start to calculate offset within .data section
                    let data_offset = if let Some(&data_target_start) =
                        merged_symbol_map.get("__data_target_start")
                    {
                        // __USER_MAIN_PTR is at user_init_ptr_vma
                        // __data_target_start is the start of .data section
                        // Offset within .data = user_init_ptr_vma - data_target_start
                        (user_init_ptr_vma - data_target_start) as usize
                    } else {
                        // Fallback: assume __USER_MAIN_PTR is at start of .data (offset 0)
                        // This matches the linker script where __USER_MAIN_PTR is first in .data
                        0
                    };

                    // The LMA is data_source_start + offset within .data section
                    let lma_offset = data_source_start as usize + data_offset;

                    if lma_offset + 4 <= code.len() {
                        // Write init address as little-endian u32 to ROM LMA
                        code[lma_offset..lma_offset + 4].copy_from_slice(&init_addr.to_le_bytes());
                    }
                }
            }
        }
    }

    debug!("=== Object file loading complete ===");
    debug!(
        "Object file: .text at 0x{:x}, .data at offset 0x{:x}",
        section_placement.text_start, section_placement.data_start
    );

    Ok(ObjectLoadInfo {
        init_address,
        symbol_map: obj_symbol_map,
        text_start: section_placement.text_start,
        data_start: section_placement.data_start,
    })
}
