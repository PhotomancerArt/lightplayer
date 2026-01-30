//! Phase 1: Analyze relocations and identify GOT entries.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use hashbrown::HashMap;
use object::{Object, ObjectSection, ObjectSymbol, RelocationFlags, RelocationTarget};

use super::got::{GotTracker, identify_got_entries};
use super::section::resolve_section_addresses;

/// Information about a relocation.
#[derive(Debug, Clone)]
pub struct RelocationInfo {
    /// Section name
    pub section_name: String,
    /// Offset within the section
    pub offset: u64,
    /// Relocation type (numeric)
    pub r_type: u32,
    /// Target symbol name
    pub symbol_name: String,
    /// Addend
    pub addend: i64,
    /// Address where relocation is applied (VMA + offset)
    pub address: u32,
    /// Section VMA
    #[allow(dead_code, reason = "Used for section address tracking")]
    pub section_vma: u64,
    /// Section LMA (for .data sections)
    #[allow(dead_code, reason = "Used for section load address tracking")]
    pub section_lma: u64,
}

/// Analyze all relocations and identify GOT entries.
pub fn analyze_relocations(
    obj: &object::File,
    _rom: &[u8],
    _ram: &[u8],
    symbol_map: &HashMap<String, u32>,
) -> Result<
    (
        Vec<RelocationInfo>,
        GotTracker,
        HashMap<String, super::section::SectionAddressInfo>,
    ),
    String,
> {
    debug!("=== Phase 1: Relocation Analysis ===");

    // Resolve section addresses
    let section_addrs = resolve_section_addresses(obj, _rom, _ram, symbol_map)?;

    // Collect all relocations
    let mut relocations = Vec::new();

    for section in obj.sections() {
        let section_name = section.name().unwrap_or("<unnamed>");

        // Skip debug sections
        if section_name.starts_with(".debug_") || section_name.starts_with(".zdebug_") {
            continue;
        }

        // Skip non-loadable sections
        match section.kind() {
            object::SectionKind::Text
            | object::SectionKind::Data
            | object::SectionKind::ReadOnlyData
            | object::SectionKind::ReadOnlyString
            | object::SectionKind::UninitializedData => {}
            _ => continue,
        }

        // Get section address info
        let section_info = match section_addrs.get(&section_name.to_string()) {
            Some(info) => info,
            None => continue, // Skip sections not in our map
        };

        // Collect relocations for this section
        let mut section_relocs = Vec::new();
        for (reloc_offset, reloc) in section.relocations() {
            // Get symbol name
            let symbol_name = match reloc.target() {
                RelocationTarget::Symbol(sym_idx) => {
                    if let Ok(sym) = obj.symbol_by_index(sym_idx) {
                        sym.name().unwrap_or("<unnamed>").to_string()
                    } else {
                        format!("symbol_index_{}", sym_idx.0)
                    }
                }
                _ => "<unknown>".to_string(),
            };

            // Get relocation type
            let r_type = match reloc.flags() {
                RelocationFlags::Elf { r_type } => r_type,
                _ => {
                    debug!(
                        "  Warning: Unsupported relocation format in section '{}' at offset 0x{:x}",
                        section_name, reloc_offset
                    );
                    continue;
                }
            };

            // Calculate address where relocation is applied
            // For .data sections, use LMA (where data is actually loaded in ROM)
            let load_addr =
                if section_info.vma >= 0x80000000 && section.kind() == object::SectionKind::Data {
                    section_info.lma
                } else {
                    section_info.vma
                };
            let address = (load_addr + reloc_offset) as u32;

            let reloc_info = RelocationInfo {
                section_name: section_name.to_string(),
                offset: reloc_offset,
                r_type,
                symbol_name,
                addend: reloc.addend(),
                address,
                section_vma: section_info.vma,
                section_lma: section_info.lma,
            };

            section_relocs.push(reloc_info);
        }

        if !section_relocs.is_empty() {
            debug!(
                "Section '{}' (VMA: 0x{:x}, LMA: 0x{:x}): {} relocations",
                section_name,
                section_info.vma,
                section_info.lma,
                section_relocs.len()
            );
            for reloc in &section_relocs {
                let r_type_str = match reloc.r_type {
                    1 => "R_RISCV_32",
                    17 => "R_RISCV_CALL_PLT",
                    19 => "R_RISCV_GOT_HI20",
                    20 => "R_RISCV_PCREL_HI20",
                    21 => "R_RISCV_PCREL_LO12_I",
                    24 => "R_RISCV_PCREL_LO12_I",
                    _ => "R_RISCV_UNKNOWN",
                };
                debug!(
                    "  Relocation at 0x{:x} (address 0x{:x}): {} → '{}' (addend: {})",
                    reloc.offset, reloc.address, r_type_str, reloc.symbol_name, reloc.addend
                );
            }
            relocations.extend(section_relocs);
        }
    }

    debug!("Total relocations found: {}", relocations.len());

    // Identify GOT entries
    let got_tracker = identify_got_entries(&relocations);

    debug!("=== GOT Entries Identified ===");
    for (name, entry) in got_tracker.entries() {
        debug!(
            "  '{}': R_RISCV_32 at 0x{:x} in '{}'",
            name,
            entry.address,
            relocations
                .iter()
                .find(|r| r.symbol_name == *name && r.r_type == 1)
                .map(|r| r.section_name.as_str())
                .unwrap_or("unknown")
        );
    }

    Ok((relocations, got_tracker, section_addrs))
}
