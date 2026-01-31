//! Symbol map building for object files.

extern crate alloc;

use ::object::{Object, ObjectSection, ObjectSymbol, SymbolSection};
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use hashbrown::HashMap;

use super::super::memory::RAM_START;

/// Build symbol map for object file with adjusted addresses.
///
/// Creates a symbol map from object file symbols, adjusting their addresses
/// based on where sections were placed in memory.
///
/// # Arguments
///
/// * `obj` - The object file to build symbol map from
/// * `text_placement` - Address where .text section was placed
/// * `data_placement` - Offset where .data section was placed (relative to RAM_START)
///
/// # Returns
///
/// Symbol map mapping symbol names to their final addresses.
pub fn build_object_symbol_map(
    obj: &::object::File,
    text_placement: u32,
    data_placement: u32,
) -> HashMap<String, u32> {
    debug!("=== Building object file symbol map ===");
    debug!(
        "Text placement: 0x{:x}, Data placement offset: 0x{:x}",
        text_placement, data_placement
    );

    let mut symbol_map: HashMap<String, u32> = HashMap::new();

    // First pass: collect all symbols, preferring defined ones
    let mut defined_symbols: Vec<(String, u32, SymbolSection)> = Vec::new();
    let mut undefined_symbols: Vec<(String, u32)> = Vec::new();

    for symbol in obj.symbols() {
        if let Ok(name) = symbol.name() {
            if name.is_empty() {
                continue; // Skip unnamed symbols
            }
            // Skip compiler-internal symbols (start with $)
            if name.starts_with('$') {
                continue;
            }

            let symbol_addr = symbol.address();
            let symbol_section = symbol.section();
            let is_defined = symbol_section != SymbolSection::Undefined;

            // Determine which section this symbol belongs to and adjust address
            let final_addr = if !is_defined {
                // Undefined symbol - keep address as-is (will be resolved via merge)
                symbol_addr as u32
            } else {
                // Defined symbol - need to find which section it belongs to
                let section_name = if let Some(section_idx) = symbol_section.index() {
                    if let Ok(section) = obj.section_by_index(section_idx) {
                        section.name().ok()
                    } else {
                        None
                    }
                } else {
                    None
                };

                match section_name {
                    Some(name) if name == ".text" || name.starts_with(".text.") => {
                        // .text section or subsection symbol: adjust by text_placement
                        // Get section VMA to determine if symbol_addr is absolute or relative
                        let section_vma = symbol_section
                            .index()
                            .and_then(|idx| obj.section_by_index(idx).ok())
                            .map(|s| s.address())
                            .unwrap_or(0);

                        // Calculate offset of this subsection within combined .text region
                        // We need to sum sizes of all .text sections before this one
                        let mut subsection_offset = 0u32;
                        if name != ".text" {
                            // This is a subsection - find its position
                            for section in obj.sections() {
                                if let Ok(sec_name) = section.name() {
                                    if sec_name == ".text" || sec_name.starts_with(".text.") {
                                        if sec_name == name {
                                            break; // Found our subsection
                                        }
                                        // Add size of this section (aligned)
                                        let sec_size = section.size() as usize;
                                        subsection_offset =
                                            (subsection_offset + sec_size as u32 + 3) & !3;
                                    }
                                }
                            }
                        }

                        if section_vma == 0 {
                            // Section starts at 0, so symbol_addr is section-relative offset
                            text_placement
                                .wrapping_add(subsection_offset)
                                .wrapping_add(symbol_addr as u32)
                        } else {
                            // Section has non-zero VMA - symbol_addr is absolute, need to subtract VMA first
                            let offset = (symbol_addr - section_vma) as u32;
                            text_placement
                                .wrapping_add(subsection_offset)
                                .wrapping_add(offset)
                        }
                    }
                    Some(name) if name == ".data" || name.starts_with(".data.") => {
                        // .data section or subsection symbol: adjust by RAM_START + data_placement
                        // Calculate offset of this subsection within combined .data region
                        let mut subsection_offset = 0u32;
                        if name != ".data" {
                            // This is a subsection - find its position
                            for section in obj.sections() {
                                if let Ok(sec_name) = section.name() {
                                    if sec_name == ".data" || sec_name.starts_with(".data.") {
                                        if sec_name == name {
                                            break; // Found our subsection
                                        }
                                        // Add size of this section (aligned)
                                        let sec_size = section.size() as usize;
                                        subsection_offset =
                                            (subsection_offset + sec_size as u32 + 3) & !3;
                                    }
                                }
                            }
                        }
                        // symbol_addr is section-relative offset
                        RAM_START
                            .wrapping_add(data_placement)
                            .wrapping_add(subsection_offset)
                            .wrapping_add(symbol_addr as u32)
                    }
                    Some(".rodata") => {
                        // .rodata section symbol: placed in code buffer after .text
                        // For now, place after .text (we'd need to track .rodata placement)
                        // This is a simplification - in practice .rodata might be placed differently
                        text_placement.wrapping_add(symbol_addr as u32)
                    }
                    Some(".bss") => {
                        // .bss section symbol: placed in RAM buffer after .data
                        // For now, place after .data (we'd need to track .bss placement)
                        RAM_START
                            .wrapping_add(data_placement)
                            .wrapping_add(symbol_addr as u32)
                    }
                    _ => {
                        // Unknown section or no section - use address as-is
                        symbol_addr as u32
                    }
                }
            };

            if is_defined {
                defined_symbols.push((name.to_string(), final_addr, symbol_section));
            } else {
                undefined_symbols.push((name.to_string(), final_addr));
            }
        }
    }

    // Add defined symbols first
    // If there are duplicates, keep the one with the higher address
    for (name, addr, _section) in defined_symbols {
        if let Some(&existing_addr) = symbol_map.get(&name) {
            if addr > existing_addr {
                symbol_map.insert(name.clone(), addr);
            }
        } else {
            symbol_map.insert(name.clone(), addr);
        }
    }

    // Add undefined symbols only if not already present
    for (name, addr) in undefined_symbols {
        if !symbol_map.contains_key(&name) {
            symbol_map.insert(name.clone(), addr);
        }
    }

    debug!("Object symbol map contains {} entries", symbol_map.len());
    symbol_map
}

/// Merge base and object symbol maps.
///
/// Combines symbol maps, with base symbols taking precedence over object symbols.
/// Detects conflicts and returns an error if a symbol exists in both maps with different addresses.
///
/// # Arguments
///
/// * `base_map` - Base executable's symbol map
/// * `obj_map` - Object file's symbol map
///
/// # Returns
///
/// Merged symbol map with base symbols taking precedence, or error if conflicts detected.
pub fn merge_symbol_maps(
    base_map: &HashMap<String, u32>,
    obj_map: &HashMap<String, u32>,
) -> Result<HashMap<String, u32>, String> {
    let mut merged = base_map.clone();
    let mut conflicts = Vec::new();

    for (name, obj_addr) in obj_map {
        if let Some(&base_addr) = base_map.get(name) {
            // Only report conflict if both symbols are defined (non-zero addresses)
            // Undefined symbols (0x0) in object file should resolve to base executable
            if base_addr != *obj_addr && *obj_addr != 0 {
                conflicts.push(format!(
                    "Symbol '{name}' conflict: base executable has 0x{base_addr:08x}, object file has 0x{obj_addr:08x}"
                ));
            }
            // Keep base version (already in merged)
        } else {
            // New symbol from object file - add it
            merged.insert(name.clone(), *obj_addr);
        }
    }

    if !conflicts.is_empty() {
        return Err(format!(
            "Symbol conflicts detected during linking:\n  {}",
            conflicts.join("\n  ")
        ));
    }

    Ok(merged)
}
