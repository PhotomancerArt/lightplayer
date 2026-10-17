//! The mask ROM's idea of the flash part (contract C3): 4 MB.
//!
//! The ROM keeps a legacy descriptor of the flash chip in its own RAM
//! (`rom_spiflash_legacy_data`, a pointer at `0x4087_FFEC` to six words:
//! device id, chip size, block size, sector size, page size, status mask).
//! Its flash routines — the ones esp-storage reads, writes and erases
//! through — refuse any address past `chip_size`. The ROM's own default is
//! smaller than the part, and the IDF bootloader corrects it from the image
//! header (`esp_rom_spiflash_config_param(id, 4 MB, 64 KiB, 4 KiB, 256,
//! 0xFFFF)`, which only stores those six words).
//!
//! Found on the emulator (2026-10-17): without this the first boot through
//! our door read the partition table and the loader (below 2 MB) and then
//! failed every read of `lpfs` at `0x35_0000`, so the core took the
//! unreadable partition for a tree store and locked the board's files. We
//! store the same words, keeping the ROM's device id (we issue no `RDID`;
//! nothing on the boot path reads the id).

const LEGACY_DATA_PTR: u32 = 0x4087_FFEC;
const CHIP_SIZE: u32 = 4 * 1024 * 1024;
const BLOCK_SIZE: u32 = 64 * 1024;
const SECTOR_SIZE: u32 = 4 * 1024;
const PAGE_SIZE: u32 = 256;
const STATUS_MASK: u32 = 0xFFFF;

pub unsafe fn four_megabytes() {
    // SAFETY: the ROM's own descriptor in its RAM, which nothing else uses
    // while this stage runs; the same stores the ROM's config routine makes.
    unsafe {
        let chip = core::ptr::read_volatile(LEGACY_DATA_PTR as *const *mut u32);
        for (word, value) in [CHIP_SIZE, BLOCK_SIZE, SECTOR_SIZE, PAGE_SIZE, STATUS_MASK]
            .into_iter()
            .enumerate()
        {
            chip.add(word + 1).write_volatile(value);
        }
    }
}
