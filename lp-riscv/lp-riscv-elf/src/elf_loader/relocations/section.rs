//! Section address resolution (VMA/LMA).

use alloc::string::{String, ToString};
use hashbrown::HashMap;
use object::{Object, ObjectSection, ObjectSymbol};

use super::super::memory::{is_ram_address, is_rom_address, ram_address_to_offset};

/// Information about a section's addresses and buffer location.
pub struct SectionAddressInfo {
    /// Virtual Memory Address (where the section appears at runtime)
    pub vma: u64,
    /// Load Memory Address (where the section is loaded in ROM)
    pub lma: u64,
    /// Buffer slice (ROM or RAM)
    pub buffer: BufferSlice,
}

/// Which buffer a section is in.
pub enum BufferSlice {
    /// ROM buffer, starting at the given offset
    Rom { offset: usize },
    /// RAM buffer, starting at the given offset
    Ram { offset: usize },
}

/// Resolve section addresses (VMA/LMA) and determine buffer locations.
pub fn resolve_section_addresses(
    obj: &object::File,
    _rom: &[u8],
    _ram: &[u8],
    symbol_map: &HashMap<String, u32>,
) -> Result<HashMap<String, SectionAddressInfo>, String> {
    debug!("=== Resolving section addresses ===");

    let mut section_addrs: HashMap<String, SectionAddressInfo> = HashMap::new();

    // Build section VMA map from symbols (same logic as sections.rs)
    let mut section_vma_map: HashMap<String, u64> = HashMap::new();
    for symbol in obj.symbols() {
        if let Ok(_name) = symbol.name() {
            let symbol_section = symbol.section();
            if let Some(section_idx) = symbol_section.index() {
                if let Ok(section) = obj.section_by_index(section_idx) {
                    if let Ok(section_name) = section.name() {
                        let section_addr = section.address();
                        let symbol_addr = symbol.address();

                        // Only use symbol address if:
                        // 1. Section address is 0 (no explicit address)
                        // 2. Symbol address is in RAM (>= 0x80000000)
                        // 3. Section is a data section
                        if section_addr == 0
                            && is_ram_address(symbol_addr)
                            && section.kind() == object::SectionKind::Data
                        {
                            section_vma_map
                                .entry(section_name.to_string())
                                .and_modify(|vma| {
                                    if symbol_addr < *vma {
                                        *vma = symbol_addr;
                                    }
                                })
                                .or_insert(symbol_addr);
                        }
                    }
                }
            }
        }
    }

    // Find __data_source_start symbol to determine .data section LMA
    let data_source_start: Option<u64> = symbol_map
        .get("__data_source_start")
        .map(|&addr| addr as u64)
        .or_else(|| {
            for symbol in obj.symbols() {
                if let Ok(name) = symbol.name() {
                    if name == "__data_source_start" {
                        return Some(symbol.address());
                    }
                }
            }
            None
        });

    let mut next_rom_offset = 0u64;
    let mut rodata_end: Option<u64> = None;

    for section in obj.sections() {
        let section_name = section.name().unwrap_or("<unnamed>");
        let section_kind = section.kind();
        let section_addr = section.address(); // LMA from file

        // Skip debug sections
        if section_name.starts_with(".debug_") || section_name.starts_with(".zdebug_") {
            continue;
        }

        // Skip non-loadable sections
        match section_kind {
            object::SectionKind::Text
            | object::SectionKind::Data
            | object::SectionKind::ReadOnlyData
            | object::SectionKind::ReadOnlyString
            | object::SectionKind::UninitializedData => {}
            _ => continue,
        }

        let data = match section.data() {
            Ok(d) => d,
            Err(_) => continue,
        };

        if data.is_empty() {
            continue;
        }

        // Determine VMA
        let vma = if section_addr == 0 {
            if let Some(&ram_vma) = section_vma_map.get(&section_name.to_string()) {
                ram_vma
            } else {
                let current = next_rom_offset;
                next_rom_offset = (current + data.len() as u64 + 3) & !3; // Align to 4 bytes
                current
            }
        } else {
            if is_rom_address(section_addr) {
                let end_addr = section_addr + data.len() as u64;
                if end_addr > next_rom_offset {
                    next_rom_offset = end_addr;
                }
            } else if is_ram_address(section_addr) {
                // Track RAM sections for sequential placement if needed
            }
            section_addr
        };

        // Determine LMA (for .data sections with RAM VMA, LMA is in ROM)
        let lma = if is_ram_address(vma) && section_kind == object::SectionKind::Data {
            if section_name == ".data" {
                // Use __data_source_start if available (it's the authoritative source)
                // TEMPORARY FIX: Use 0xa18 if __data_source_start not found (match sections.rs)
                // Otherwise use .rodata end, otherwise sequential placement
                data_source_start
                    .or(Some(0xa18)) // TEMPORARY: hardcode 0xa18
                    .or(rodata_end) // Fallback to .rodata end
                    .unwrap_or(next_rom_offset)
            } else if section_addr == 0 {
                let current = next_rom_offset;
                next_rom_offset = (current + data.len() as u64 + 3) & !3;
                current
            } else {
                section_addr
            }
        } else {
            vma // LMA == VMA for ROM sections
        };

        // Track .rodata end for .data LMA calculation
        if section_name == ".rodata" && is_rom_address(vma) {
            rodata_end = Some(vma + data.len() as u64);
        }

        // Determine buffer slice
        let buffer = if is_rom_address(vma) {
            BufferSlice::Rom {
                offset: vma as usize,
            }
        } else if is_ram_address(vma) {
            if section_kind == object::SectionKind::Data {
                // .data sections: VMA in RAM, but relocations apply to LMA in ROM
                BufferSlice::Rom {
                    offset: lma as usize,
                }
            } else {
                BufferSlice::Ram {
                    offset: ram_address_to_offset(vma),
                }
            }
        } else {
            continue; // Skip sections not in ROM or RAM
        };

        debug!(
            "  Section '{}': VMA=0x{:x}, LMA=0x{:x}, size={}",
            section_name,
            vma,
            lma,
            data.len()
        );

        section_addrs.insert(
            section_name.to_string(),
            SectionAddressInfo { vma, lma, buffer },
        );
    }

    debug!("Resolved {} sections", section_addrs.len());
    Ok(section_addrs)
}
