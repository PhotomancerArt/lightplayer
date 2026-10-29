//! The size a SPI flash chip's JEDEC id names.
//!
//! The id is three bytes as esptool reports it, `0x00MMTTCC`: manufacturer,
//! memory type, and a capacity byte that is log2 of the size in bytes
//! (`0x16` = 4 MB, `0x17` = 8 MB, `0x18` = 16 MB). It is how a board's flash
//! size is told without trusting the board's label — the XIAO ESP32-C6
//! carries 4 MB, the C6 DevKitC-1 8 MB, on the same chip.
//!
//! An id of zero, all ones, or a capacity outside what any ESP32 module
//! carries is an answer too: **unknown**. esptool-js's id probe reads 0 on an
//! ESP32-C6 rev 2 over USB-Serial-JTAG while stub reads and writes work
//! (bench, 2026-07-31), so "unknown" is a normal outcome there, never an
//! error.

/// The smallest capacity byte believed (256 KB).
const MIN_CAPACITY_LOG2: u32 = 0x12;
/// The largest capacity byte believed (128 MB).
const MAX_CAPACITY_LOG2: u32 = 0x1B;

/// The flash size in bytes the JEDEC `id` names, or `None` when the id is
/// not one a real part reports.
pub fn flash_size_from_jedec_id(id: u32) -> Option<u64> {
    if id == 0 || id & 0x00FF_FFFF == 0x00FF_FFFF {
        return None;
    }
    let capacity = (id >> 16) & 0xFF;
    (MIN_CAPACITY_LOG2..=MAX_CAPACITY_LOG2)
        .contains(&capacity)
        .then(|| 1u64 << capacity)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIB: u64 = 1024 * 1024;

    #[test]
    fn the_capacity_byte_names_the_size() {
        // Winbond W25Q32 (4 MB), W25Q64 (8 MB), W25Q128 (16 MB); XMC 4 MB.
        assert_eq!(flash_size_from_jedec_id(0x0016_40EF), Some(4 * MIB));
        assert_eq!(flash_size_from_jedec_id(0x0017_40EF), Some(8 * MIB));
        assert_eq!(flash_size_from_jedec_id(0x0018_4020), Some(16 * MIB));
        assert_eq!(flash_size_from_jedec_id(0x0016_4020), Some(4 * MIB));
    }

    #[test]
    fn a_probe_that_read_nothing_is_unknown() {
        assert_eq!(flash_size_from_jedec_id(0), None);
        assert_eq!(flash_size_from_jedec_id(0x00FF_FFFF), None);
        assert_eq!(flash_size_from_jedec_id(0xFFFF_FFFF), None);
    }

    #[test]
    fn a_capacity_no_module_carries_is_unknown() {
        assert_eq!(flash_size_from_jedec_id(0x0001_40EF), None);
        assert_eq!(flash_size_from_jedec_id(0x0030_40EF), None);
    }
}
