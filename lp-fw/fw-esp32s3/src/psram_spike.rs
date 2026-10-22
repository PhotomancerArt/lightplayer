//! PSRAM spike (feature `psram-spike`, never shipped): the 8 MB octal PSRAM
//! as a heap region of the app, under a build-time policy.
//!
//! `LP_PSRAM_POLICY` (build time):
//! - `init` — bring PSRAM up, add no region (the boot cost alone);
//! - `last` — an `External` region after every internal one: Rust overflows
//!   into it only when internal RAM cannot serve a request;
//! - `all` — every Rust allocation tries PSRAM first (code, data, buffers);
//! - `split:<n>` — requests of at least `n` bytes try PSRAM first, smaller
//!   ones internal first.
//!
//! The radio's C heap and esp-rtos's stacks ask for `Internal` and never land
//! in PSRAM under any policy. JIT code that lands in PSRAM is published with
//! a D-cache write-back and an I-cache invalidate (`lpvm_native::XT_CODE_PUBLISH`)
//! and runs through the I-bus twin of its page.

use core::sync::atomic::Ordering;

use esp_hal::psram::{Psram, PsramConfig, PsramMode, PsramSize, SpiRamFreq};

const EXT_DBUS: usize = 0x3C00_0000;
const EXT_IBUS: usize = 0x4200_0000;

unsafe extern "C" {
    fn rom_Cache_WriteBack_Addr(addr: u32, size: u32);
    fn Cache_Invalidate_Addr(addr: u32, size: u32) -> i32;
    fn Cache_Suspend_DCache_Autoload() -> u32;
    fn Cache_Resume_DCache_Autoload(value: u32);
}

static INIT: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(0);
static BASE: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(0);
static SAID: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);

pub fn policy() -> &'static str {
    option_env!("LP_PSRAM_POLICY").unwrap_or("init")
}

/// Bring PSRAM up and install the policy. Logged with `esp_println` (the
/// link is not up yet), like the other `[INIT]` lines.
pub fn install() {
    let t0 = esp_hal::time::Instant::now();
    // SAFETY: nothing else in this image takes the PSRAM singleton.
    let peri = unsafe { esp_hal::peripherals::PSRAM::steal() };
    let freq = match option_env!("LP_PSRAM_MHZ") {
        Some("40") => SpiRamFreq::Freq40m,
        _ => SpiRamFreq::Freq80m,
    };
    let psram = Psram::new(
        peri,
        PsramConfig {
            mode: PsramMode::OctalSpi,
            size: PsramSize::AutoDetect,
            ram_frequency: freq,
            ..Default::default()
        },
    );
    let (base, size) = psram.raw_parts();
    core::mem::forget(psram);
    let us = t0.elapsed().as_micros();
    INIT.store(us as u32, Ordering::Relaxed);
    BASE.store(base as u32, Ordering::Relaxed);
    esp_println::println!(
        "[INIT] psram: {size} B at {:#010x} ({:?}), up in {us} us, policy {}, MMU entries {:#x}..{:#x}",
        base as usize,
        freq,
        policy(),
        (base as usize).saturating_sub(EXT_DBUS) / 0x1_0000,
        (base as usize + size).saturating_sub(EXT_DBUS) / 0x1_0000
    );
    if size == 0 {
        return;
    }
    lpvm_native::XT_CODE_PUBLISH.store(publish_code as usize, Ordering::Relaxed);
    let p = policy();
    if p == "init" {
        return;
    }
    // SAFETY: the mapped PSRAM is ours alone for the life of the program.
    unsafe {
        esp_alloc::HEAP.add_region(esp_alloc::HeapRegion::new(
            base,
            size,
            esp_alloc::MemoryCapability::External.into(),
        ));
    }
    let min = match p {
        "all" => 0,
        "last" => usize::MAX,
        s if s.starts_with("split:") => s[6..].parse().unwrap_or(usize::MAX),
        _ => usize::MAX,
    };
    esp_alloc::EXTERNAL_FIRST_MIN.store(min, Ordering::Relaxed);
}

fn publish_code(write: usize, len: usize) {
    // SAFETY: mask-ROM cache routines over a range of mapped PSRAM.
    unsafe {
        let autoload = Cache_Suspend_DCache_Autoload();
        rom_Cache_WriteBack_Addr(write as u32, len as u32);
        Cache_Resume_DCache_Autoload(autoload);
        Cache_Invalidate_Addr((write - EXT_DBUS + EXT_IBUS) as u32, len as u32);
    }
}

/// One heartbeat line: where the heap is, and what the LED output saw.
pub fn log_heartbeat() {
    use esp_alloc::MemoryCapability::{External, Internal};
    if !SAID.swap(true, Ordering::Relaxed) {
        log::info!(
            "[psram] up at {:#010x} in {} us at boot, policy {}, clock {}",
            BASE.load(Ordering::Relaxed),
            INIT.load(Ordering::Relaxed),
            policy(),
            option_env!("LP_PSRAM_MHZ").unwrap_or("80")
        );
    }
    let internal = esp_alloc::HEAP.free_caps(Internal.into());
    let external = esp_alloc::HEAP.free_caps(External.into());
    let mut frames = 0;
    let mut trips = 0;
    let mut errors = 0;
    let mut over_half = 0;
    for ch in 0..4u8 {
        let s = crate::output::rmt::shared_driver::DRIVER.stats(ch);
        frames += s.frames;
        trips += s.guard_trips;
        errors += s.errors;
        over_half += s.entry_delay_over_half();
    }
    let stats = esp_alloc::HEAP.stats();
    let mut regions = alloc::string::String::new();
    for r in stats.region_stats.iter().flatten() {
        use core::fmt::Write;
        let _ = write!(regions, " {}/{}", r.used, r.size);
    }
    log::info!(
        "[psram] t={}ms free internal={internal} external={external} regions(used/size):{regions} | led frames={frames} trips={trips} errors={errors} entry_over_half={over_half}",
        esp_hal::time::Instant::now().duration_since_epoch().as_millis()
    );
}
