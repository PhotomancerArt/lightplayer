//! Memory layout calculation for ELF loading.

use super::memory::{RAM_START, is_ram_address, is_rom_address};
use alloc::string::String;
use object::{Object, ObjectSection};

/// Memory layout information for ELF loading.
pub struct MemoryLayout {
    /// Size of ROM buffer needed (in bytes)
    pub rom_size: usize,
    /// Size of RAM buffer needed (in bytes)
    pub ram_size: usize,
    /// Entry point address
    #[allow(unused, reason = "Reserved for future use in entry point handling")]
    pub entry_point: u32,
}

/// Calculate memory layout based on section addresses.
pub fn calculate_memory_layout(
    obj: &object::File,
    entry_point: u32,
) -> Result<MemoryLayout, String> {
    debug!("=== Calculating memory layout ===");

    let mut max_rom_addr: u64 = 0;
    let mut max_ram_addr: u64 = 0;

    // Iterate through all sections to determine buffer sizes
    for section in obj.sections() {
        let section_name = section.name().unwrap_or("<unnamed>");
        let section_addr = section.address();
        let section_size = section.size();

        // Skip debug sections
        if section_name.starts_with(".debug_") || section_name.starts_with(".zdebug_") {
            continue;
        }

        // Skip sections with no address (linker script symbols, etc.)
        if section_addr == 0 && section_size == 0 {
            continue;
        }

        debug!(
            "  Section '{}': addr=0x{:x}, size={}",
            section_name, section_addr, section_size
        );

        if is_rom_address(section_addr) {
            let end_addr = section_addr + section_size;
            max_rom_addr = max_rom_addr.max(end_addr);
        } else if is_ram_address(section_addr) {
            let ram_offset = section_addr - RAM_START as u64;
            let ram_end = ram_offset + section_size;
            max_ram_addr = max_ram_addr.max(ram_end);
        }
    }

    // Allocate buffers with padding
    // ROM: at least 4KB + 4KB padding for PC-relative loads
    let rom_size = (max_rom_addr.max(4096) + 4096) as usize;
    // RAM: at least 512KB for heap/stack
    let ram_size = (max_ram_addr.max(512 * 1024)) as usize;

    debug!(
        "Calculated layout: ROM={} bytes (max addr: 0x{:x}), RAM={} bytes (max offset: 0x{:x})",
        rom_size, max_rom_addr, ram_size, max_ram_addr
    );

    Ok(MemoryLayout {
        rom_size,
        ram_size,
        entry_point,
    })
}
