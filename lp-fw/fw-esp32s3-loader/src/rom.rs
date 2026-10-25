//! The mask-ROM routines the loader uses (addresses in `loader.x`, the S3
//! ROM's jump table). None of them touches SPI1.

unsafe extern "C" {
    fn ets_printf(fmt: *const core::ffi::c_char, ...) -> i32;
    fn rtc_get_reset_reason(cpu: u32) -> u32;
    fn Cache_Invalidate_ICache_All();
    fn Cache_Invalidate_DCache_All();
}

/// The reset-reason code of this boot, PRO core (classified by
/// `lp_bootctl::ResetKind::from_s3_reason`).
pub fn reset_reason() -> u32 {
    // SAFETY: a ROM routine reading a status register.
    unsafe { rtc_get_reset_reason(0) }
}

/// Both caches hold lines read through mappings that just changed.
pub fn invalidate_caches() {
    // SAFETY: ROM routines with no arguments; nothing runs from flash.
    unsafe {
        Cache_Invalidate_DCache_All();
        Cache_Invalidate_ICache_All();
    }
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
