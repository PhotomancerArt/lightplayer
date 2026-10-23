//! The octal PSRAM (feature `psram`, off by default): brought up once in the
//! core's boot, at a fixed MMU window, and checked before anything may use it.
//!
//! # Where, and why there
//!
//! At [`PSRAM_WINDOW`] (entries `0x80..0x100`; data `0x3C80_0000`, code
//! `0x4280_0000`, 8 MiB) on every S3 image, plain or split, through the
//! esp-hal fork's `mmu_start_entry` (its seventh diff). The window is carved
//! out of the top of the core's entries, so it is clear of the engine,
//! `lpfs` and the core's scratch (`lp_bootctl::s3_mmu`); upstream's own choice
//! does not fit on the split image at all.
//!
//! # When it comes up, and when it does not
//!
//! In the core, right after the heap is installed and before the filesystem
//! maps `lpfs` or a split image maps its engine: the core must reach PSRAM
//! whether or not the engine runs. Only on a part whose eFuse says it carries
//! **8 MB** of PSRAM (`PSRAM_CAP` 1 — ESP-IDF's S3 eFuse table — which is the
//! S3R8's octal part; a 2 MB part is quad and 16 MB is the 1.8 V octal part,
//! neither of which this window or this config is for), in octal mode, at
//! the window's whole 8 MiB. Anything else — a part without PSRAM, a driver
//! that finds none, a self-test that fails — boots exactly as an image
//! without the feature does, and [`State`] says which, in one
//! `[INIT] psram:` line.
//!
//! # The self-test
//!
//! A few milliseconds, never the whole memtest (0.5 s at 80 MHz in the
//! spike): one compare-and-swap (atomics on PSRAM are what an `Arc` in a
//! PSRAM heap relies on, and esp-alloc warns they may not work), then a
//! sampled pattern — one word in every 4 KiB, address-derived so a page
//! mapped twice or at the wrong place reads back another sample's value —
//! written, read back and restored to zero.
//!
//! # The heap
//!
//! Once up, the window is one esp-alloc region tagged `External` only
//! ([`add_heap_region`]), registered after every internal one, and where
//! Rust's default allocations go is the build-time knob `LP_PSRAM_HEAP`
//! ([`HeapPolicy`]), until the walk's numbers settle it (M10's Q-M10a):
//!
//! | `LP_PSRAM_HEAP` | the region | Rust's default allocations |
//! |---|---|---|
//! | `all` (default) | yes | PSRAM first, every size |
//! | `split:<bytes>` | yes | PSRAM first at or above `<bytes>`, internal first below |
//! | `last` | yes | internal first; PSRAM only once internal is full |
//! | `off` | no | internal only, as an image without the feature |
//!
//! Whatever asks for `Internal` — the radio's C heap, esp-rtos's thread
//! stacks, esp-radio's DMA buffers — never lands here under any policy, and
//! the load gates' figures ([`internal_free`], [`internal_largest_block`])
//! stay internal RAM's: that is the scarce one.

use core::sync::atomic::{AtomicU8, AtomicU32, Ordering};

use esp_hal::psram::{Psram, PsramConfig, PsramMode, PsramSize, SpiRamFreq};
use lp_bootctl::s3_mmu::{PSRAM_DROM, PSRAM_MMU_ENTRY, PSRAM_WINDOW_LEN};

pub use lp_bootctl::s3_mmu::PSRAM_WINDOW;

/// What this boot's bring-up came to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    /// Not tried yet (before `core_boot` reaches it).
    NotTried,
    /// The eFuse says this part has no 8 MB PSRAM: `PSRAM_CAP` as read.
    NotFused(u8),
    /// The driver found no octal PSRAM where the eFuse said one was.
    DriverFoundNone,
    /// The driver mapped something other than the window.
    WrongWindow,
    /// The compare-and-swap did not behave.
    CasFailed,
    /// A sampled word read back wrong.
    PatternFailed,
    /// Up, tested, and usable: the whole window.
    Up,
}

impl State {
    fn code(self) -> u8 {
        match self {
            State::NotTried => 0,
            State::NotFused(_) => 1,
            State::DriverFoundNone => 2,
            State::WrongWindow => 3,
            State::CasFailed => 4,
            State::PatternFailed => 5,
            State::Up => 6,
        }
    }

    fn of(code: u8) -> Self {
        match code {
            1 => State::NotFused(0),
            2 => State::DriverFoundNone,
            3 => State::WrongWindow,
            4 => State::CasFailed,
            5 => State::PatternFailed,
            6 => State::Up,
            _ => State::NotTried,
        }
    }

    /// The `[INIT] psram:` line's words.
    pub fn describe(self) -> &'static str {
        match self {
            State::NotTried => "not tried",
            State::NotFused(_) => "off: the eFuse says this part has no 8 MB PSRAM",
            State::DriverFoundNone => "off: the driver found no octal PSRAM",
            State::WrongWindow => "off: the driver mapped it outside the window",
            State::CasFailed => "off: the self-test's compare-and-swap failed",
            State::PatternFailed => "off: the self-test's pattern read back wrong",
            State::Up => "on",
        }
    }
}

static STATE: AtomicU8 = AtomicU8::new(0);

/// This boot's state (the heartbeat and the allocator read it).
pub fn state() -> State {
    State::of(STATE.load(Ordering::Acquire))
}

/// Whether PSRAM is up and passed its self-test this boot.
pub fn usable() -> bool {
    state() == State::Up
}

/// The window's data-bus range, when [`usable`].
pub fn window() -> Option<(usize, usize)> {
    usable().then_some((PSRAM_DROM as usize, PSRAM_WINDOW_LEN as usize))
}

/// esp-hal's eFuse field for the PSRAM's capacity, read whole: bits 131..133
/// and its high bit at 179 (`PSRAM_CAP`, `PSRAM_CAP_3`).
fn fused_capacity() -> u8 {
    use esp_hal::efuse::{PSRAM_CAP, PSRAM_CAP_3, read_bit, read_field_le};
    read_field_le::<u8>(PSRAM_CAP) | (u8::from(read_bit(PSRAM_CAP_3)) << 2)
}

/// PSRAM's clock: `LP_PSRAM_MHZ` as built, 80 (the default) or 40, the
/// fallback the spike named for a part with too little margin at 80 (esp-hal
/// tunes no MSPI timing). At 80 MHz esp-hal's init sets the MSPI core clock to
/// 160 MHz and divides both flash and PSRAM by two, so the flash is at 80 MHz
/// as well; at 40 the core stays at 80 and the flash is undivided.
fn ram_frequency() -> SpiRamFreq {
    match option_env!("LP_PSRAM_MHZ") {
        Some("40") => SpiRamFreq::Freq40m,
        _ => SpiRamFreq::Freq80m,
    }
}

/// `PSRAM_CAP`'s code for 8 MB (ESP-IDF's S3 eFuse table: `1: "8M"`).
const FUSED_8MB: u8 = 1;

static PERIPHERAL: critical_section::Mutex<
    core::cell::RefCell<Option<esp_hal::peripherals::PSRAM<'static>>>,
> = critical_section::Mutex::new(core::cell::RefCell::new(None));

/// Keep the PSRAM peripheral for [`bring_up`]. Called from `init_board`, the
/// one place the peripheral singleton is split (BLE's `stash`'s shape).
pub fn stash(peri: esp_hal::peripherals::PSRAM<'static>) {
    critical_section::with(|cs| PERIPHERAL.borrow_ref_mut(cs).replace(peri));
}

/// Bring PSRAM up: the eFuse gate, the driver at the window, the self-test.
/// Prints the `[INIT] psram:` line. Called once, from `core_boot`.
pub fn bring_up() -> State {
    let state = match critical_section::with(|cs| PERIPHERAL.borrow_ref_mut(cs).take()) {
        Some(peri) => try_bring_up(peri),
        None => State::DriverFoundNone,
    };
    STATE.store(state.code(), Ordering::Release);
    match state {
        State::Up => esp_println::println!(
            "[INIT] psram: on, {PSRAM_WINDOW_LEN} bytes at {PSRAM_DROM:#010x} (MMU entries \
             {:#x}..{:#x}), self-test passed",
            PSRAM_WINDOW.start,
            PSRAM_WINDOW.end
        ),
        State::NotFused(cap) => {
            esp_println::println!("[INIT] psram: {} (PSRAM_CAP={cap})", state.describe())
        }
        other => esp_println::println!("[INIT] psram: {}", other.describe()),
    }
    state
}

fn try_bring_up(peri: esp_hal::peripherals::PSRAM<'static>) -> State {
    let cap = fused_capacity();
    if cap != FUSED_8MB {
        return State::NotFused(cap);
    }
    // `new_octal`: the quad path is never referenced, so none of its `#[ram]`
    // code is linked (the fork's seventh diff).
    let config = PsramConfig {
        ram_frequency: ram_frequency(),
        mode: PsramMode::OctalSpi,
        size: PsramSize::Size(PSRAM_WINDOW_LEN as usize),
        mmu_start_entry: Some(PSRAM_MMU_ENTRY),
        ..PsramConfig::default()
    };
    let psram = Psram::new_octal(peri, config);
    let (ptr, len) = psram.raw_parts();
    // The mapping outlives the handle (esp-hal's `Psram` has no `Drop`), and
    // nothing else may claim the peripheral again.
    core::mem::forget(psram);
    if len == 0 {
        return State::DriverFoundNone;
    }
    if ptr as usize != PSRAM_DROM as usize || len != PSRAM_WINDOW_LEN as usize {
        return State::WrongWindow;
    }
    self_test(ptr as usize, len)
}

/// One sample per this many bytes.
const SAMPLE_STRIDE: usize = 4096;

/// See the module docs.
fn self_test(base: usize, len: usize) -> State {
    // SAFETY: the window was just mapped, 4-aligned, and nothing else holds
    // a reference into it yet.
    let cas = unsafe { &*(base as *const AtomicU32) };
    cas.store(0x5A5A_0001, Ordering::SeqCst);
    let swapped =
        cas.compare_exchange(0x5A5A_0001, 0xA5A5_0002, Ordering::SeqCst, Ordering::SeqCst);
    let refused =
        cas.compare_exchange(0x5A5A_0001, 0xDEAD_BEEF, Ordering::SeqCst, Ordering::SeqCst);
    if swapped != Ok(0x5A5A_0001)
        || refused != Err(0xA5A5_0002)
        || cas.load(Ordering::SeqCst) != 0xA5A5_0002
    {
        return State::CasFailed;
    }
    cas.store(0, Ordering::SeqCst);

    let word = |i: usize| (base + i * SAMPLE_STRIDE + 4) as *mut u32;
    let pattern = |i: usize| (i as u32).wrapping_mul(0x9E37_79B1) ^ 0x5EED_5EED;
    let samples = len / SAMPLE_STRIDE;
    for i in 0..samples {
        // SAFETY: inside the mapped window, 4-aligned.
        unsafe { core::ptr::write_volatile(word(i), pattern(i)) };
    }
    let mut ok = true;
    for i in 0..samples {
        // SAFETY: as above.
        ok &= unsafe { core::ptr::read_volatile(word(i)) } == pattern(i);
        unsafe { core::ptr::write_volatile(word(i), 0) };
    }
    if ok { State::Up } else { State::PatternFailed }
}

/// Where Rust's default allocations go (module docs, "The heap").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HeapPolicy {
    /// No PSRAM region at all.
    Off,
    /// The region, used only once the internal ones are full.
    Last,
    /// PSRAM first at or above this many bytes (`0` = every allocation).
    FirstFrom(usize),
}

impl HeapPolicy {
    /// `LP_PSRAM_HEAP` as built: `all` when unset or unreadable.
    pub fn built() -> Self {
        Self::parse(option_env!("LP_PSRAM_HEAP").unwrap_or("all"))
            .unwrap_or(HeapPolicy::FirstFrom(0))
    }

    fn parse(text: &str) -> Option<Self> {
        match text {
            "all" => Some(HeapPolicy::FirstFrom(0)),
            "last" => Some(HeapPolicy::Last),
            "off" => Some(HeapPolicy::Off),
            other => other
                .strip_prefix("split:")
                .and_then(|n| n.parse().ok())
                .map(HeapPolicy::FirstFrom),
        }
    }
}

/// Register the window as the heap's `External` region and set the
/// placement threshold, when PSRAM is [`usable`] and the policy is not
/// [`HeapPolicy::Off`]. After every internal region, so first-fit in
/// registration order reaches PSRAM last on the default path. Prints the
/// `[INIT] heap region:` line it adds, or why it adds none.
pub fn add_heap_region() {
    let Some((base, len)) = window() else {
        return;
    };
    let policy = HeapPolicy::built();
    if policy == HeapPolicy::Off {
        esp_println::println!("[INIT] heap region: none in PSRAM (LP_PSRAM_HEAP=off)");
        return;
    }
    let used = esp_alloc::HEAP
        .stats()
        .region_stats
        .iter()
        .filter(|r| r.is_some())
        .count();
    if used >= esp_alloc::MAX_REGIONS {
        esp_println::println!(
            "[INIT] heap region: none in PSRAM — esp-alloc's {} regions are all taken",
            esp_alloc::MAX_REGIONS
        );
        return;
    }
    // SAFETY: the window is mapped, tested, zeroed where the test wrote, and
    // handed to the allocator exactly once, here; nothing else holds it.
    unsafe {
        esp_alloc::HEAP.add_region(esp_alloc::HeapRegion::new(
            base as *mut u8,
            len,
            esp_alloc::MemoryCapability::External.into(),
        ));
    }
    let threshold = match policy {
        HeapPolicy::FirstFrom(n) => n,
        _ => usize::MAX,
    };
    esp_alloc::EXTERNAL_FIRST_MIN.store(threshold, Ordering::Relaxed);
    match policy {
        HeapPolicy::FirstFrom(0) => esp_println::println!(
            "[INIT] heap region: {base:#010x}+{len} (PSRAM, External; Rust's allocations go here first)"
        ),
        HeapPolicy::FirstFrom(n) => esp_println::println!(
            "[INIT] heap region: {base:#010x}+{len} (PSRAM, External; Rust's allocations of {n} B or more go here first)"
        ),
        _ => esp_println::println!(
            "[INIT] heap region: {base:#010x}+{len} (PSRAM, External; used once internal RAM is full)"
        ),
    }
}

fn external() -> enumset::EnumSet<esp_alloc::MemoryCapability> {
    esp_alloc::MemoryCapability::External.into()
}

/// Free bytes in internal RAM: every heap region that is not PSRAM.
pub fn internal_free() -> usize {
    esp_alloc::HEAP.free_excluding(external())
}

/// Used bytes in internal RAM.
pub fn internal_used() -> usize {
    region_totals(false).1
}

/// `(free, used)` in PSRAM's region (zeros when there is none).
pub fn psram_totals() -> (usize, usize) {
    let (free, used) = region_totals(true);
    (free, used)
}

fn region_totals(psram: bool) -> (usize, usize) {
    esp_alloc::HEAP
        .stats()
        .region_stats
        .iter()
        .flatten()
        .filter(|r| {
            r.capabilities
                .contains(esp_alloc::MemoryCapability::External)
                == psram
        })
        .fold((0, 0), |(f, u), r| (f + r.free, u + r.used))
}

/// The largest block internal RAM can give, exact to the byte: what the load
/// gates read.
pub fn internal_largest_block() -> usize {
    largest_block(internal_free(), |layout| unsafe {
        // SAFETY: a non-zero layout, freed at once by the caller.
        esp_alloc::HEAP.alloc_excluding(external(), layout)
    })
}

/// The largest block PSRAM's region can give.
pub fn psram_largest_block() -> usize {
    largest_block(psram_totals().0, |layout| unsafe {
        // SAFETY: as above.
        esp_alloc::HEAP.alloc_caps(external(), layout)
    })
}

fn largest_block(bound: usize, alloc: impl Fn(core::alloc::Layout) -> *mut u8) -> usize {
    fw_esp32_common::largest_block::largest_fitting(bound, |size| {
        if size == 0 {
            return true;
        }
        let Ok(layout) = core::alloc::Layout::from_size_align(size, 4) else {
            return false;
        };
        // `black_box`: an allocation only freed may otherwise be deleted
        // (docs/defects/2026-10-09-the-s3s-largest-block-probe-was-optimized-away.md).
        let ptr = core::hint::black_box(alloc(layout));
        if ptr.is_null() {
            false
        } else {
            // SAFETY: freed with the layout it was allocated with.
            unsafe { alloc::alloc::dealloc(ptr, layout) };
            true
        }
    })
}

/// The ledger's PSRAM line, after `[MEM]` (which is internal RAM's on a
/// `psram` image).
pub fn log_ledger_line() {
    if !usable() {
        log::info!("[PSRAM] {}", state().describe());
        return;
    }
    let (free, used) = psram_totals();
    if free + used == 0 {
        log::info!("[PSRAM] on, not in the heap");
        return;
    }
    let largest = psram_largest_block();
    log::info!("[PSRAM] free={free} used={used} largest_free={largest}");
}

unsafe extern "C" {
    fn Cache_WriteBack_All() -> i32;
    /// The S3 mask ROM's range invalidate (`esp32s3.rom.ld`); 0 = done.
    fn Cache_Invalidate_Addr(vaddr: u32, size: u32) -> i32;
}

/// The cache's line.
const CACHE_LINE: usize = 32;

/// `lpvm_native::code_publish`'s routine on this chip: write the data cache
/// back, so the code reaches PSRAM, then drop the instruction cache's lines
/// over the code's fetch range, so the next fetch reads it. The whole data
/// cache (32 KB) is cheaper to name than to walk, once per shader.
pub fn publish_code(_write: usize, exec: usize, len: usize) {
    let start = exec & !(CACHE_LINE - 1);
    let end = (exec + len).next_multiple_of(CACHE_LINE);
    core::sync::atomic::compiler_fence(Ordering::SeqCst);
    // SAFETY: ROM routines; the range is the buffer's own fetch addresses.
    unsafe {
        Cache_WriteBack_All();
        Cache_Invalidate_Addr(start as u32, (end - start) as u32);
    }
    core::sync::atomic::compiler_fence(Ordering::SeqCst);
}

/// Whether `addr` is in the PSRAM window (data bus).
pub fn in_window(addr: usize) -> bool {
    let base = PSRAM_DROM as usize;
    addr >= base && addr < base + PSRAM_WINDOW_LEN as usize
}

/// A byte buffer in internal RAM — any heap region but PSRAM's — that grows
/// to the largest frame it was asked to hold. What the RMT output copies a
/// PSRAM frame into (`output::rmt`), so its refill ISR reads internal RAM.
pub struct InternalBuffer {
    ptr: *mut u8,
    cap: usize,
}

impl InternalBuffer {
    pub const fn empty() -> Self {
        Self {
            ptr: core::ptr::null_mut(),
            cap: 0,
        }
    }

    /// `src` copied into the buffer, or `None` when internal RAM cannot hold
    /// it.
    pub fn copy_of(&mut self, src: &[u8]) -> Option<&[u8]> {
        if src.len() > self.cap {
            self.release();
            let layout = core::alloc::Layout::from_size_align(src.len(), 4).ok()?;
            // SAFETY: a non-zero layout (an empty frame never needs a copy
            // larger than `cap = 0`, so `src.len() > 0` here).
            let ptr = unsafe { esp_alloc::HEAP.alloc_excluding(external(), layout) };
            if ptr.is_null() {
                return None;
            }
            self.ptr = ptr;
            self.cap = src.len();
        }
        // SAFETY: `cap >= src.len()` bytes at `ptr` are this buffer's alone.
        unsafe {
            core::ptr::copy_nonoverlapping(src.as_ptr(), self.ptr, src.len());
            Some(core::slice::from_raw_parts(self.ptr, src.len()))
        }
    }

    fn release(&mut self) {
        if !self.ptr.is_null() {
            // SAFETY: allocated in `copy_of` with exactly this layout.
            unsafe {
                alloc::alloc::dealloc(
                    self.ptr,
                    core::alloc::Layout::from_size_align_unchecked(self.cap, 4),
                )
            };
            self.ptr = core::ptr::null_mut();
            self.cap = 0;
        }
    }
}

impl Drop for InternalBuffer {
    fn drop(&mut self) {
        self.release();
    }
}
