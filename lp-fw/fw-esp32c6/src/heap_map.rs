//! Diagnostic: the heap's free holes and live spans, by address.
//!
//! Printed through `log`, so it rides the link's log channel to whatever
//! host is connected (`lp-cli upload` prints it as `[serial]` lines); raw
//! `esp_println` text does not reach a host on an lp-link USB port.
//!
//! Feature `heap_map_diag`; **never shipped**. Written for the defect
//! `2026-09-24-ble-enabled-c6-refuses-a-project-switch-after-the-heap-cut`:
//! the load gate refuses on the largest free block, and which live
//! allocation splits the main region is a question of *addresses*, which no
//! counter the firmware keeps can answer.
//!
//! The map is taken by asking the allocator, not by reading its internals:
//! with interrupts masked, repeatedly allocate the largest block that fits
//! (a binary search), keep it, and record where it landed, until nothing of
//! [`MIN_HOLE`] B fits; then free them all. What remains between the
//! recorded holes, inside each region's bounds, is live (or a hole smaller
//! than [`MIN_HOLE`]).
//!
//! With `heap_track_diag` as well, every allocation in the main region or
//! `dram2_seg` made after [`arm_tracking`] (the station arms it when it
//! joins) is remembered with its frame-pointer backtrace while it is live,
//! and the map prints the live ones once they are few (after a project
//! stops) — `addr2line` on the ELF names the allocator. The table lives in
//! LP SRAM, so the heap's layout is the shipped image's.

use core::alloc::Layout;
use core::sync::atomic::{AtomicU32, Ordering};

/// Holes smaller than this are not reported (they are counted as live).
const MIN_HOLE: usize = 32;
/// At most this many holes per map.
const MAX_HOLES: usize = 48;
/// Minimum spacing between two periodic maps.
const PERIOD_MS: u32 = 5_000;

static LAST_MAP_MS: AtomicU32 = AtomicU32::new(0);

/// Log a map at most every [`PERIOD_MS`], tagged `tag`.
pub fn log_periodic(tag: &str) {
    let now = embassy_time::Instant::now().as_millis() as u32;
    let last = LAST_MAP_MS.load(Ordering::Relaxed);
    if last != 0 && now.saturating_sub(last) < PERIOD_MS {
        return;
    }
    LAST_MAP_MS.store(now.max(1), Ordering::Relaxed);
    log(tag);
}

/// Log the map now, tagged `tag`.
pub fn log(tag: &str) {
    let mut holes = [(0usize, 0usize); MAX_HOLES];
    let n = critical_section::with(|_| take_holes(&mut holes));
    let holes = &mut holes[..n];
    holes.sort_unstable_by_key(|(addr, _)| *addr);

    let regions = crate::board::esp32c6::init::heap_regions();
    log::info!(
        "[heapmap] {tag}: used {} free {} holes {n} (>= {MIN_HOLE} B)",
        esp_alloc::HEAP.used(),
        esp_alloc::HEAP.free()
    );
    for (ri, (start, size)) in regions.iter().enumerate() {
        let end = start + size;
        log::info!("[heapmap] {tag}: region {ri} 0x{start:08x}..0x{end:08x} ({size} B)");
        let mut cursor = *start;
        for (addr, len) in holes.iter().filter(|(a, _)| *a >= *start && *a < end) {
            if *addr > cursor {
                log::info!(
                    "[heapmap] {tag}:   live 0x{cursor:08x}..0x{addr:08x} {} B (+{})",
                    addr - cursor,
                    cursor - start
                );
            }
            log::info!(
                "[heapmap] {tag}:   HOLE 0x{addr:08x}..0x{:08x} {len} B (+{})",
                addr + len,
                addr - start
            );
            cursor = addr + len;
        }
        if cursor < end {
            log::info!(
                "[heapmap] {tag}:   live 0x{cursor:08x}..0x{end:08x} {} B (+{})",
                end - cursor,
                cursor - start
            );
        }
    }
    #[cfg(feature = "heap_track_diag")]
    track::log_live(tag);
}

/// Allocate the largest block that fits, again and again; free them all.
/// Must run with interrupts masked, so nothing else allocates in between.
fn take_holes(out: &mut [(usize, usize); MAX_HOLES]) -> usize {
    #[cfg(feature = "heap_track_diag")]
    track::pause(true);
    let mut n = 0;
    while n < MAX_HOLES {
        let size = largest_fit();
        if size < MIN_HOLE {
            break;
        }
        let Ok(layout) = Layout::from_size_align(size, 4) else {
            break;
        };
        // SAFETY: size > 0; freed below with the same layout.
        let ptr = unsafe { alloc::alloc::alloc(layout) };
        if ptr.is_null() {
            break;
        }
        out[n] = (ptr as usize, size);
        n += 1;
    }
    for (addr, size) in out[..n].iter().rev() {
        // SAFETY: allocated above with exactly this layout.
        unsafe {
            alloc::alloc::dealloc(
                *addr as *mut u8,
                Layout::from_size_align_unchecked(*size, 4),
            )
        };
    }
    #[cfg(feature = "heap_track_diag")]
    track::pause(false);
    n
}

/// The largest 4-aligned allocation that succeeds, to 4 B.
fn largest_fit() -> usize {
    let mut too_big = esp_alloc::HEAP.free() + 1;
    let mut fits = 0usize;
    while too_big - fits > 4 {
        let mid = fits + (too_big - fits) / 2;
        let Ok(layout) = Layout::from_size_align(mid, 4) else {
            break;
        };
        // SAFETY: mid > 0; freed at once with the same layout.
        let ptr = unsafe { alloc::alloc::alloc(layout) };
        if ptr.is_null() {
            too_big = mid;
        } else {
            unsafe { alloc::alloc::dealloc(ptr, layout) };
            fits = mid;
        }
    }
    fits
}

/// Arm allocation tracking (`heap_track_diag`) from now on: forget what was
/// recorded and record every heap allocation made after this call, in the
/// main region and `dram2_seg`, while it lives. A no-op without the feature.
/// The station calls it when it joins, so what the network allocates while a
/// project runs (and what outlives the project) is what the table holds.
pub fn arm_tracking(why: &str) {
    #[cfg(feature = "heap_track_diag")]
    track::arm(why);
    #[cfg(not(feature = "heap_track_diag"))]
    let _ = why;
}

/// Allocation tracking (`heap_track_diag`): once [`arm_tracking`] has run,
/// every allocation in the main region or `dram2_seg` is remembered with its
/// backtrace while it lives. The table sits in LP SRAM (`rtc_fast`), which the
/// firmware barely uses, so the build's heap layout is the shipped one: the
/// regions keep their sizes and their addresses.
#[cfg(feature = "heap_track_diag")]
mod track {
    use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

    const SLOTS: usize = 300;
    const FRAMES: usize = 8;
    /// Frames to skip: the hook itself and `EspHeap::alloc_caps`.
    const SKIP: usize = 2;

    #[derive(Clone, Copy)]
    struct Entry {
        addr: u32,
        size: u32,
        frames: [u32; FRAMES],
    }

    const EMPTY: Entry = Entry {
        addr: 0,
        size: 0,
        frames: [0; FRAMES],
    };

    #[esp_hal::ram(unstable(rtc_fast))]
    static mut TABLE: [Entry; SLOTS] = [EMPTY; SLOTS];
    static ARMED: AtomicBool = AtomicBool::new(false);
    static PAUSED: AtomicBool = AtomicBool::new(false);
    static OVERFLOW: AtomicU32 = AtomicU32::new(0);

    fn table(_cs: critical_section::CriticalSection<'_>) -> &'static mut [Entry; SLOTS] {
        // SAFETY: only ever reached inside a critical section (the token), so
        // there is one borrow at a time.
        unsafe { &mut *core::ptr::addr_of_mut!(TABLE) }
    }

    pub fn arm(why: &str) {
        critical_section::with(|cs| {
            // Slot by slot: a whole-array assignment would build its 12 KB
            // on the caller's stack (the net thread has 8 KB).
            for slot in table(cs).iter_mut() {
                *slot = EMPTY;
            }
            OVERFLOW.store(0, Ordering::Relaxed);
            ARMED.store(true, Ordering::Relaxed);
        });
        log::info!("[heaptrack] armed: {why}");
    }

    pub fn pause(on: bool) {
        PAUSED.store(on, Ordering::Relaxed);
    }

    fn tracked(ptr: usize) -> bool {
        let regions = crate::board::esp32c6::init::heap_regions();
        regions[..2]
            .iter()
            .any(|(start, size)| ptr >= *start && ptr < start + size)
    }

    #[unsafe(no_mangle)]
    fn _esp_alloc_alloc(
        _heap: &esp_alloc::EspHeap,
        _caps: enumset::EnumSet<esp_alloc::MemoryCapability>,
        ptr: usize,
        size: usize,
    ) {
        if !ARMED.load(Ordering::Relaxed) || PAUSED.load(Ordering::Relaxed) || ptr == 0 {
            return;
        }
        if !tracked(ptr) {
            return;
        }
        let mut raw = [0u32; FRAMES + SKIP];
        lpc_shared::backtrace::capture_frames(&mut raw);
        let mut frames = [0u32; FRAMES];
        frames.copy_from_slice(&raw[SKIP..]);
        critical_section::with(|cs| match table(cs).iter_mut().find(|e| e.addr == 0) {
            Some(slot) => {
                *slot = Entry {
                    addr: ptr as u32,
                    size: size as u32,
                    frames,
                }
            }
            None => {
                OVERFLOW.fetch_add(1, Ordering::Relaxed);
            }
        });
    }

    #[unsafe(no_mangle)]
    fn _esp_alloc_dealloc(_heap: &esp_alloc::EspHeap, ptr: usize, _size: usize) {
        if ptr == 0 || !ARMED.load(Ordering::Relaxed) {
            return;
        }
        critical_section::with(|cs| {
            if let Some(slot) = table(cs).iter_mut().find(|e| e.addr == ptr as u32) {
                slot.addr = 0;
            }
        });
    }

    /// Print the live tracked allocations — only when there are few (after a
    /// project stops), never the hundreds a running project holds.
    pub fn log_live(tag: &str) {
        if !ARMED.load(Ordering::Relaxed) {
            return;
        }
        let live = critical_section::with(|cs| table(cs).iter().filter(|e| e.addr != 0).count());
        log::info!(
            "[heaptrack] {tag}: {live} live since armed, overflow {}",
            OVERFLOW.load(Ordering::Relaxed)
        );
        if live > 96 {
            return;
        }
        for i in 0..SLOTS {
            let e = critical_section::with(|cs| table(cs)[i]);
            if e.addr == 0 {
                continue;
            }
            let f = e.frames;
            log::info!(
                "[heaptrack] {tag}: 0x{:08x} {} B frames {:08x} {:08x} {:08x} {:08x} {:08x} {:08x} {:08x} {:08x}",
                e.addr,
                e.size,
                f[0],
                f[1],
                f[2],
                f[3],
                f[4],
                f[5],
                f[6],
                f[7]
            );
        }
    }
}
