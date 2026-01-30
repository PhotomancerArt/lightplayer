//! Phase 2: Apply relocations in dependency order.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use hashbrown::HashMap;

use super::got::GotTracker;
use super::handlers::{
    RelocationContext, handle_call_plt, handle_got_hi20, handle_pcrel_hi20, handle_pcrel_lo12_i,
};
use super::phase1::RelocationInfo;
use super::section::SectionAddressInfo;

/// Apply relocations in dependency order.
/// GOT entries (R_RISCV_32) are applied first, then references to them.
pub fn apply_relocations_phase2(
    relocations: &[RelocationInfo],
    got_tracker: &GotTracker,
    section_addrs: &HashMap<String, SectionAddressInfo>,
    rom: &mut [u8],
    ram: &mut [u8],
    symbol_map: &HashMap<String, u32>,
) -> Result<(), String> {
    debug!("=== Phase 2: Applying Relocations ===");

    // We need a mutable reference to got_tracker for marking entries as initialized
    // So we'll create a mutable copy
    let mut got_tracker = got_tracker.clone();

    // Separate relocations by type: GOT entries first, then others
    let mut got_relocs = Vec::new();
    let mut other_relocs = Vec::new();

    for reloc in relocations {
        if reloc.r_type == 1 && got_tracker.has_entry(&reloc.symbol_name) {
            // R_RISCV_32 that initializes a GOT entry
            got_relocs.push(reloc.clone());
        } else {
            // Other relocations
            other_relocs.push(reloc.clone());
        }
    }

    debug!(
        "Applying {} GOT entry relocations first, then {} other relocations",
        got_relocs.len(),
        other_relocs.len()
    );

    // Apply GOT entry relocations first
    for reloc in &got_relocs {
        apply_single_relocation(
            reloc,
            &mut got_tracker,
            section_addrs,
            rom,
            ram,
            symbol_map,
            relocations,
        )?;
    }

    // Apply other relocations
    for reloc in &other_relocs {
        apply_single_relocation(
            reloc,
            &mut got_tracker,
            section_addrs,
            rom,
            ram,
            symbol_map,
            relocations,
        )?;
    }

    debug!("=== All relocations applied ===");
    Ok(())
}

/// Apply a single relocation.
fn apply_single_relocation(
    reloc: &RelocationInfo,
    got_tracker: &mut GotTracker,
    section_addrs: &HashMap<String, SectionAddressInfo>,
    rom: &mut [u8],
    ram: &mut [u8],
    symbol_map: &HashMap<String, u32>,
    all_relocations: &[RelocationInfo],
) -> Result<(), String> {
    // Get target symbol address
    let target_addr = symbol_map.get(&reloc.symbol_name)
        .copied()
        .ok_or_else(|| format!(
            "Could not resolve relocation target '{}' at offset 0x{:x}. Symbol not found in symbol map.",
            reloc.symbol_name, reloc.offset
        ))?;

    // Get section address info
    // Try direct lookup first, then try normalized name (for subsections like .text._init -> .text)
    let section_info = section_addrs
        .get(&reloc.section_name)
        .or_else(|| {
            // Normalize section name (e.g., ".text._init" -> ".text") for lookup
            let normalized = if let Some(second_dot) = reloc.section_name[1..].find('.') {
                reloc.section_name[..1 + second_dot].to_string()
            } else {
                return None;
            };
            section_addrs.get(&normalized)
        })
        .ok_or_else(|| {
            format!(
                "Section '{}' not found in section address map",
                reloc.section_name
            )
        })?;

    // Determine which buffer to use and get slice
    let (buffer_slice, load_addr) = match &section_info.buffer {
        super::section::BufferSlice::Rom { offset } => {
            if *offset + reloc.offset as usize >= rom.len() {
                return Err(format!(
                    "Relocation offset 0x{:x} in section '{}' out of bounds (rom.len()={})",
                    *offset + reloc.offset as usize,
                    reloc.section_name,
                    rom.len()
                ));
            }
            let slice = &mut rom[*offset..];
            (slice, section_info.lma)
        }
        super::section::BufferSlice::Ram { offset } => {
            if *offset + reloc.offset as usize >= ram.len() {
                return Err(format!(
                    "Relocation offset 0x{:x} in section '{}' out of bounds (ram.len()={})",
                    *offset + reloc.offset as usize,
                    reloc.section_name,
                    ram.len()
                ));
            }
            let slice = &mut ram[*offset..];
            (slice, section_info.vma)
        }
    };

    // Calculate PC (load address + offset)
    let pc = (load_addr + reloc.offset) as u32;

    // Apply relocation based on type
    match reloc.r_type {
        1 => {
            // R_RISCV_32
            // For R_RISCV_32, we need mutable access to got_tracker
            // Create a temporary context without got_tracker, then pass got_tracker separately
            let offset = reloc.offset as usize;
            if offset + 4 > buffer_slice.len() {
                return Err(format!(
                    "R_RISCV_32 relocation at offset {offset} requires 4 bytes"
                ));
            }

            // Write the absolute target address directly
            let reloc_bytes = &mut buffer_slice[offset..offset + 4];
            reloc_bytes.copy_from_slice(&target_addr.to_le_bytes());

            // If this is a GOT entry, mark it as initialized
            if got_tracker.has_entry(&reloc.symbol_name) {
                got_tracker.mark_initialized(&reloc.symbol_name);
                debug!(
                    "  Applied R_RISCV_32 at 0x{:x}: ✓ GOT entry initialized: '{}' = 0x{:x}",
                    reloc.address, reloc.symbol_name, target_addr
                );
            } else {
                debug!(
                    "  Applied R_RISCV_32 at 0x{:x}: Wrote 0x{:x} to offset 0x{:x} for '{}'",
                    reloc.address, target_addr, offset, reloc.symbol_name
                );
            }
        }
        17 => {
            // R_RISCV_CALL_PLT
            let mut ctx = RelocationContext {
                buffer: buffer_slice,
                pc,
                target_addr,
                got_tracker,
                symbol_map,
                all_relocations: Some(all_relocations),
            };
            handle_call_plt(&mut ctx, reloc).map_err(|e| {
                format!(
                    "Failed to apply R_RISCV_CALL_PLT at 0x{:x}: {}",
                    reloc.address, e
                )
            })?;
        }
        19 => {
            // R_RISCV_GOT_HI20
            let mut ctx = RelocationContext {
                buffer: buffer_slice,
                pc,
                target_addr,
                got_tracker,
                symbol_map,
                all_relocations: Some(all_relocations),
            };
            handle_got_hi20(&mut ctx, reloc).map_err(|e| {
                format!(
                    "Failed to apply R_RISCV_GOT_HI20 at 0x{:x}: {}",
                    reloc.address, e
                )
            })?;
        }
        20 => {
            // R_RISCV_PCREL_HI20
            let mut ctx = RelocationContext {
                buffer: buffer_slice,
                pc,
                target_addr,
                got_tracker,
                symbol_map,
                all_relocations: Some(all_relocations),
            };
            handle_pcrel_hi20(&mut ctx, reloc).map_err(|e| {
                format!(
                    "Failed to apply R_RISCV_PCREL_HI20 at 0x{:x}: {}",
                    reloc.address, e
                )
            })?;
        }
        21 | 24 => {
            // R_RISCV_PCREL_LO12_I
            let mut ctx = RelocationContext {
                buffer: buffer_slice,
                pc,
                target_addr,
                got_tracker,
                symbol_map,
                all_relocations: Some(all_relocations),
            };
            handle_pcrel_lo12_i(&mut ctx, reloc).map_err(|e| {
                format!(
                    "Failed to apply R_RISCV_PCREL_LO12_I at 0x{:x}: {}",
                    reloc.address, e
                )
            })?;
        }
        _ => {
            return Err(format!(
                "Unsupported relocation type {} at offset 0x{:x} in section '{}'. \
                 Supported types: R_RISCV_CALL_PLT (17), R_RISCV_PCREL_HI20 (20), \
                 R_RISCV_PCREL_LO12_I (21/24), R_RISCV_32 (1), R_RISCV_GOT_HI20 (19)",
                reloc.r_type, reloc.offset, reloc.section_name
            ));
        }
    }

    Ok(())
}
