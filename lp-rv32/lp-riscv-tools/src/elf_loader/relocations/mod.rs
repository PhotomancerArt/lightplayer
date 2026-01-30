//! Relocation application for ELF loading.
//!
//! This module implements a two-phase relocation system:
//! - Phase 1: Analyze all relocations and identify GOT entries
//! - Phase 2: Apply relocations in dependency order

mod got;
mod handlers;
mod phase1;
mod phase2;
mod section;

pub use phase1::analyze_relocations;
pub use phase2::apply_relocations_phase2;
pub use section::{BufferSlice, SectionAddressInfo};

use alloc::string::String;
use hashbrown::HashMap;

/// Apply all relocations from the ELF to ROM and RAM buffers.
pub fn apply_relocations(
    obj: &object::File,
    rom: &mut [u8],
    ram: &mut [u8],
    symbol_map: &HashMap<String, u32>,
) -> Result<(), String> {
    debug!("=== Applying relocations ===");

    // Phase 1: Analyze relocations and identify GOT entries
    let (relocations, got_tracker, section_addrs) =
        phase1::analyze_relocations(obj, rom, ram, symbol_map)?;

    // Phase 2: Apply relocations
    phase2::apply_relocations_phase2(
        &relocations,
        &got_tracker,
        &section_addrs,
        rom,
        ram,
        symbol_map,
    )?;

    debug!("=== Relocations applied successfully ===");
    Ok(())
}
