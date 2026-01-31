//! Memory region constants and utilities for ELF loading.

/// RAM start address (0x80000000, matching embive's RAM_OFFSET).
pub const RAM_START: u32 = 0x80000000;

/// Determine if an address is in ROM region.
#[inline]
pub fn is_rom_address(addr: u64) -> bool {
    addr < RAM_START as u64
}

/// Determine if an address is in RAM region.
#[inline]
pub fn is_ram_address(addr: u64) -> bool {
    addr >= RAM_START as u64
}

/// Convert a RAM address to an offset within the RAM buffer.
#[inline]
pub fn ram_address_to_offset(addr: u64) -> usize {
    (addr - RAM_START as u64) as usize
}

/// Convert a RAM offset to an absolute address.
#[inline]
#[allow(unused, reason = "Utility function for address conversion")]
pub fn ram_offset_to_address(offset: usize) -> u64 {
    RAM_START as u64 + offset as u64
}
