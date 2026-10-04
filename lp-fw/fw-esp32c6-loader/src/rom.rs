//! The mask-ROM routines the loader uses (addresses in `loader.x`,
//! from the C6 ROM's own linker script).

unsafe extern "C" {
    fn ets_printf(fmt: *const core::ffi::c_char, ...) -> i32;
    fn Cache_Invalidate_ICache_All();
    fn rtc_get_reset_reason(cpu: u32) -> u32;
}

/// The reset-reason code of this boot (classified by
/// `lp_bootctl::ResetKind::from_c6_reason`).
pub fn reset_reason() -> u32 {
    // SAFETY: a ROM routine reading a status register.
    unsafe { rtc_get_reset_reason(0) }
}

/// The flash cache holds lines the bootloader read through the old mapping.
pub fn invalidate_cache() {
    // SAFETY: a ROM routine with no arguments; nothing runs from the window.
    unsafe { Cache_Invalidate_ICache_All() }
}

/// The one line before the core runs: which core, and why this one.
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

/// A core that did not load, and why.
pub fn print_skipped(core_off: u32, why: &core::ffi::CStr) {
    // SAFETY: as above.
    unsafe {
        ets_printf(
            c"[LOADER] core @0x%x skipped: %s\n".as_ptr(),
            core_off,
            why.as_ptr(),
        )
    };
}

/// No core loaded. The ROM's USB download mode is the way back.
pub fn print_nothing_to_boot() {
    // SAFETY: a C format string with no arguments.
    unsafe { ets_printf(c"[LOADER] no core loaded — reflash over USB\n".as_ptr()) };
}
