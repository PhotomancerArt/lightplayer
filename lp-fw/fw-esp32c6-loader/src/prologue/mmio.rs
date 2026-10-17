//! Register access for the prologue: plain volatile words.

pub unsafe fn read(addr: u32) -> u32 {
    // SAFETY: the caller names a peripheral register.
    unsafe { core::ptr::read_volatile(addr as *const u32) }
}

pub unsafe fn write(addr: u32, value: u32) {
    // SAFETY: the caller names a peripheral register.
    unsafe { core::ptr::write_volatile(addr as *mut u32, value) }
}

pub unsafe fn modify(addr: u32, clear: u32, set: u32) {
    // SAFETY: as above.
    unsafe { write(addr, (read(addr) & !clear) | set) }
}
