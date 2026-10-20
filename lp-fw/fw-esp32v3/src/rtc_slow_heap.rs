//! RTC slow memory — 8 KiB this image never touched — as the allocator's
//! fifth and last region (feature `heap-rtc-slow`, on by default).
//!
//! `rtc_slow_seg` is `0x5000_0000`, 8 KiB (`third_party/esp-hal/ld/esp32/memory.x`).
//! Before this region the image placed nothing there: the ledger read all of
//! it idle (E1, 2026-10-09). The array lives in `.rtc_slow.persistent`
//! (`NOLOAD`, never cleared), so it costs no flash and no boot time.
//!
//! # Why RTC slow and not the RTC fast tail
//!
//! The classic's RTC fast memory has 7,164 idle bytes after the recovery
//! ledger, but **only the PRO core can reach it** (esp-hal's own `memory.x`:
//! "Only for core 0 (PRO_CPU)"). This firmware's APP core runs the WS281x
//! pusher, and the pusher reads every frame's bytes through a pointer the PRO
//! core posts (`lp_ws281x::WireMailbox::frame_ptr`) — bytes that come from the
//! heap. A frame that landed in RTC fast would be unreadable from the core
//! that sends it. esp-alloc cannot keep one kind of allocation out of a region
//! the global allocator may use, so the RTC fast tail stays out of the heap.
//! RTC slow is on both cores' data buses.
//!
//! # Who else uses these bytes, and when
//!
//! Nobody, on any path this image takes: neither the mask ROM ELF
//! (`lp-emu/esp/roms/esp32_rev300_rom.elf`) nor espflash 3.3.0's
//! `esp32-bootloader.bin` (loads `0x3FFF_0030..`, `0x4007_8000..`,
//! `0x4008_0400..`) places anything in `0x5000_0000..0x5000_2000`, and this
//! image runs no ULP program. Nothing in it DMAs.
//!
//! # Why last
//!
//! RTC slow memory may be slower to reach than SRAM (not measured here; a
//! desk check). esp-alloc is first-fit in registration order, so as the last
//! region it takes only what the four SRAM regions cannot fit — overflow,
//! never the boot residents or the project's first allocations. It carries no
//! capability tag, so a request that asks for `Internal` does not land here
//! either.

/// `rtc_slow_seg`'s length.
pub const RTC_SLOW_HEAP_BYTES: usize = 8 * 1024;

/// `persistent` takes a `Persistable` type, which `MaybeUninit` is not; the
/// initializer is never written (the section is `NOLOAD` and not cleared).
#[esp_hal::ram(unstable(rtc_slow, persistent))]
static mut HEAP_RTC_SLOW: [u8; RTC_SLOW_HEAP_BYTES] = [0; RTC_SLOW_HEAP_BYTES];

/// Hand RTC slow memory to the allocator and return `(start address, size)`.
/// Call it after every SRAM region is registered.
pub fn add_region() -> (usize, usize) {
    let base = core::ptr::addr_of_mut!(HEAP_RTC_SLOW).cast::<u8>();
    // SAFETY: the span is `'static`, exclusively the allocator's (this is the
    // only `.rtc_slow.*` static in the image; nothing else reads or writes
    // `0x5000_0000..0x5000_2000` — see the module docs), and non-empty. A heap
    // needs no contents to survive a reset, so `persistent` (not cleared) is
    // fine.
    unsafe {
        esp_alloc::HEAP.add_region(esp_alloc::HeapRegion::new(
            base,
            RTC_SLOW_HEAP_BYTES,
            Default::default(),
        ));
    }
    (base as usize, RTC_SLOW_HEAP_BYTES)
}
