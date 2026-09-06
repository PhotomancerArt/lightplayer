//! `critical-section` implementation for emulator guests.
//!
//! Guest programs are single-threaded and have no interrupts — the
//! emulator only ever executes one instruction stream — so a critical
//! section is a no-op. This satisfies crates (e.g. `lp-recovery`) that
//! guard shared state with `critical_section::with`.

struct GuestCriticalSection;

critical_section::set_impl!(GuestCriticalSection);

unsafe impl critical_section::Impl for GuestCriticalSection {
    unsafe fn acquire() -> critical_section::RawRestoreState {}

    unsafe fn release(_restore_state: critical_section::RawRestoreState) {}
}
