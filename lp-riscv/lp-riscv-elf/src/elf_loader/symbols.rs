//! Symbol map building for relocations.

use super::memory::is_ram_address;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use hashbrown::HashMap;
use object::{Object, ObjectSymbol, SymbolSection};

/// Build a comprehensive symbol map for relocations.
///
/// Returns a map from symbol name to address (offset for ROM symbols, absolute for RAM symbols).
pub fn build_symbol_map(obj: &object::File, text_base: u64) -> HashMap<String, u32> {
    log::debug!("=== Building symbol map for relocations ===");
    log::debug!("text_section_base: 0x{text_base:x}");

    let mut symbol_map: HashMap<String, u32> = HashMap::new();

    // First pass: collect all symbols, preferring defined ones
    let mut defined_symbols: Vec<(String, u32, SymbolSection)> = Vec::new();
    let mut undefined_symbols: Vec<(String, u32)> = Vec::new();

    for symbol in obj.symbols() {
        if let Ok(name) = symbol.name() {
            if name.is_empty() {
                continue; // Skip unnamed symbols
            }

            let addr = symbol.address();
            let symbol_section = symbol.section();
            let is_defined = symbol_section != SymbolSection::Undefined;

            // Calculate offset/address
            // For RAM symbols, use absolute address
            // For ROM symbols, use offset relative to text base
            let offset = if is_ram_address(addr) {
                // RAM symbol - use absolute address
                addr as u32
            } else if addr >= text_base {
                // ROM symbol - relative to text base
                (addr - text_base) as u32
            } else {
                // Other ROM sections - use as-is
                addr as u32
            };

            if is_defined {
                defined_symbols.push((name.to_string(), offset, symbol_section));
            } else {
                undefined_symbols.push((name.to_string(), offset));
            }
        }
    }

    // Add defined symbols first
    // If there are duplicates, keep the one with the higher address
    for (name, offset, section) in defined_symbols {
        if let Some(&existing_offset) = symbol_map.get(&name) {
            if offset > existing_offset {
                symbol_map.insert(name.clone(), offset);
                log::trace!(
                    "  Symbol '{name}': replacing offset 0x{existing_offset:x} with 0x{offset:x} (higher address), section={section:?}"
                );
            } else {
                log::trace!(
                    "  Symbol '{name}': keeping existing offset 0x{existing_offset:x} (new: 0x{offset:x}), section={section:?}"
                );
            }
        } else {
            symbol_map.insert(name.clone(), offset);
            log::trace!("  Symbol '{name}': offset=0x{offset:x}, section={section:?} (defined)");
        }
    }

    // Add undefined symbols only if not already present
    for (name, offset) in undefined_symbols {
        if !symbol_map.contains_key(&name) {
            symbol_map.insert(name.clone(), offset);
            log::trace!("  Symbol '{name}': offset=0x{offset:x} (undefined)");
        } else {
            log::trace!("  Symbol '{name}': skipping undefined (already have defined)");
        }
    }

    log::debug!("Symbol map contains {} entries", symbol_map.len());
    symbol_map
}

/// Find the address of a symbol by name.
///
/// Returns the absolute address of the symbol (for both ROM and RAM symbols).
pub fn find_symbol_address(
    obj: &object::File,
    symbol_name: &str,
    _text_section_base: u64,
) -> Result<u32, String> {
    for symbol in obj.symbols() {
        // Don't filter by symbol kind - absolute address symbols (like __data_source_start)
        // are not Text symbols, but we still need to find them
        if let Ok(name) = symbol.name() {
            if name == symbol_name {
                let addr = symbol.address();
                // Return the absolute address directly
                // For absolute address symbols (like __data_source_start), addr is already absolute
                // For section-relative symbols, addr is relative to the section, but we want absolute
                // For now, just return the address as-is since linker-provided symbols are usually absolute
                return Ok(addr as u32);
            }
        }
    }
    Err(format!("Symbol '{symbol_name}' not found"))
}
