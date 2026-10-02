//! The mask-ROM routines the loader uses (addresses in `loader.x`,
//! from the C6 ROM's own linker script).

unsafe extern "C" {
    fn ets_printf(fmt: *const core::ffi::c_char, ...) -> i32;
    fn Cache_Invalidate_ICache_All();
    fn rtc_get_reset_reason(cpu: u32) -> u32;
}

/// This boot follows a power-on or a brownout (`0x01`, `0x0F`), not a reset
/// the chip did to itself — see `lp_bootctl::choose`.
pub fn cold_boot() -> bool {
    // SAFETY: a ROM routine reading a status register.
    matches!(unsafe { rtc_get_reset_reason(0) }, 0x01 | 0x0F)
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
