//! The three mask-ROM routines the loader uses (addresses in `loader.x`,
//! from the C6 ROM's own linker script).

unsafe extern "C" {
    fn ets_printf(fmt: *const core::ffi::c_char, ...) -> i32;
    fn esp_rom_spiflash_read(src_addr: u32, dest: *mut u32, len: i32) -> i32;
    fn Cache_Invalidate_ICache_All();
}

/// Read `dest.len() * 4` bytes of flash at `addr` (4-byte aligned).
pub fn flash_read(addr: u32, dest: &mut [u32]) -> bool {
    flash_read_raw(addr, dest.as_mut_ptr(), (dest.len() * 4) as u32)
}

/// Read `len` bytes of flash at `addr` into `dest` (both 4-byte aligned).
pub fn flash_read_raw(addr: u32, dest: *mut u32, len: u32) -> bool {
    // SAFETY: the ROM SPI read; the caller owns `dest..dest+len`.
    unsafe { esp_rom_spiflash_read(addr, dest, len as i32) == 0 }
}

/// The flash cache holds lines the bootloader read through the old mapping.
pub fn invalidate_cache() {
    // SAFETY: a ROM routine with no arguments; nothing runs from the window.
    unsafe { Cache_Invalidate_ICache_All() }
}

pub fn print_core(core_off: u32, note: &core::ffi::CStr) {
    // SAFETY: a C format string with matching arguments.
    unsafe {
        ets_printf(
            c"[LOADER] core @0x%x (%s)\n".as_ptr(),
            core_off,
            note.as_ptr(),
        )
    };
}

pub fn print_failure(core_off: u32, why: &core::ffi::CStr) {
    // SAFETY: as above.
    unsafe {
        ets_printf(
            c"[LOADER] core @0x%x NOT loaded: %s\n".as_ptr(),
            core_off,
            why.as_ptr(),
        )
    };
}
