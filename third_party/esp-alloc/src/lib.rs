//! A `no_std` heap allocator for RISC-V and Xtensa processors from
//! Espressif. Supports all currently available ESP32 devices.
//!
//! **NOTE:** using this as your global allocator requires using Rust 1.68 or
//! greater, or the `nightly` release channel.
//!
//! # Using this as your Global Allocator
//!
//! ```rust,no_run
//! use esp_alloc as _;
//!
//! fn init_heap() {
//!     const HEAP_SIZE: usize = 32 * 1024;
//!     static mut HEAP: MaybeUninit<[u8; HEAP_SIZE]> = MaybeUninit::uninit();
//!
//!     unsafe {
//!         esp_alloc::HEAP.add_region(esp_alloc::HeapRegion::new(
//!             HEAP.as_mut_ptr() as *mut u8,
//!             HEAP_SIZE,
//!             esp_alloc::MemoryCapability::Internal.into(),
//!         ));
//!     }
//! }
//! ```
//!
//! Alternatively, you can use the `heap_allocator!` macro to configure the
//! global allocator with a given size:
//!
//! ```rust,no_run
//! esp_alloc::heap_allocator!(size: 32 * 1024);
//! ```
//!
//! # Using this with the nightly `allocator_api`-feature
//!
//! Sometimes you want to have more control over allocations.
//!
//! For that, it's convenient to use the nightly `allocator_api`-feature,
//! which allows you to specify an allocator for single allocations.
//!
//! **NOTE:** To use this, you have to enable the crate's `nightly` feature
//! flag.
//!
//! Create and initialize an allocator to use in single allocations:
//!
//! ```rust,no_run
//! static PSRAM_ALLOCATOR: esp_alloc::EspHeap = esp_alloc::EspHeap::empty();
//!
//! fn init_psram_heap() {
//!     unsafe {
//!         PSRAM_ALLOCATOR.add_region(esp_alloc::HeapRegion::new(
//!             psram::psram_vaddr_start() as *mut u8,
//!             psram::PSRAM_BYTES,
//!             esp_alloc::MemoryCapability::External.into(),
//!         ));
//!     }
//! }
//! ```
//!
//! And then use it in an allocation:
//!
//! ```rust,no_run
//! let large_buffer: Vec<u8, _> = Vec::with_capacity_in(1048576, &PSRAM_ALLOCATOR);
//! ```
//!
//! Alternatively, you can use the `psram_allocator!` macro to configure the
//! global allocator to use PSRAM:
//!
//! ```rust,no_run
//! let p = esp_hal::init(esp_hal::Config::default());
//! esp_alloc::psram_allocator!(p.PSRAM, esp_hal::psram);
//! ```
//!
//! You can also use the `ExternalMemory` allocator to allocate PSRAM memory
//! with the global allocator:
//!
//! ```rust,no_run
//! let p = esp_hal::init(esp_hal::Config::default());
//! esp_alloc::psram_allocator!(p.PSRAM, esp_hal::psram);
//!
//! let mut vec = Vec::<u32>::new_in(esp_alloc::ExternalMemory);
//! ```
//!
//! ## `allocator_api` feature on stable Rust
//!
//! `esp-alloc` implements the allocator trait from [`allocator_api2`], which
//! provides the nightly-only `allocator_api` features in stable Rust. The crate
//! contains implementations for `Box` and `Vec`.
//!
//! To use the `allocator_api2` features, you need to add the crate to your
//! `Cargo.toml`. Note that we do not enable the `alloc` feature by default, but
//! you will need it for the `Box` and `Vec` types.
//!
//! ```toml
//! allocator-api2 = { version = "0.3", default-features = false, features = ["alloc"] }
//! ```
//!
//! With this, you can use the `Box` and `Vec` types from `allocator_api2`, with
//! `esp-alloc` allocators:
//!
//! ```rust,no_run
//! let p = esp_hal::init(esp_hal::Config::default());
//! esp_alloc::heap_allocator!(size: 64000);
//! esp_alloc::psram_allocator!(p.PSRAM, esp_hal::psram);
//!
//! let mut vec: Vec<u32, _> = Vec::new_in(esp_alloc::InternalMemory);
//!
//! vec.push(0xabcd1234);
//! assert_eq!(vec[0], 0xabcd1234);
//! ```
//!
//! Note that if you use the nightly `allocator_api` feature, you can use the
//! `Box` and `Vec` types from `alloc`. `allocator_api2` is still available as
//! an option, but types from `allocator_api2` are not compatible with the
//! standard library types.
//!
//! # Heap stats
//!
//! You can also get stats about the heap usage at anytime with:
//!
//! ```rust,no_run
//! let stats: HeapStats = esp_alloc::HEAP.stats();
//! // HeapStats implements the Display and defmt::Format traits, so you can
//! // pretty-print the heap stats.
//! println!("{}", stats);
//! ```
//!
//! Example output:
//!
//! ```txt
//! HEAP INFO
//! Size: 131068
//! Current usage: 46148
//! Max usage: 46148
//! Total freed: 0
//! Total allocated: 46148
//! Memory Layout:
//! Internal | ████████████░░░░░░░░░░░░░░░░░░░░░░░ | Used: 35% (Used 46148 of 131068, free: 84920)
//! ```
//! ## Feature Flags
#![doc = document_features::document_features!(feature_label = r#"<span class="stab portability"><code>{feature}</code></span>"#)]
#![no_std]
#![cfg_attr(feature = "nightly", feature(allocator_api))]
#![doc(html_logo_url = "https://avatars.githubusercontent.com/u/46717278")]

mod allocators;
mod heap;
mod macros;
#[cfg(feature = "compat")]
mod malloc;

use core::{
    alloc::{GlobalAlloc, Layout},
    fmt::Display,
    ptr::{self, NonNull},
};

/// Re-exported crates.
pub mod export {
    pub use enumset;
}

pub use allocators::*;
use enumset::{EnumSet, EnumSetType};
use esp_sync::NonReentrantMutex;

use crate::heap::Heap;

/// Maximum number of memory regions one allocator can hold.
///
/// ⚠️ **LP fork.** Upstream 0.10.0 hard-codes `3` in four places (the region
/// array, its initializer, `HeapStats::region_stats` and the local that fills
/// it); this fork names the cap once and raises it to 5. The classic ESP32
/// firmware registers four — the `dram_seg` arena, the SRAM1 tail, and the two
/// ROM boot-stack spans esp-hal reserves and the ROM stops using once both
/// cores are up — and `add_region` panics past the cap. Naming it also removes
/// the index hazard: `stats()` writes `region_stats[id]` for every occupied
/// slot, so the two arrays must be the same length. See `README-LP.md`.
pub const MAX_REGIONS: usize = 5;

/// The global allocator instance
#[cfg_attr(feature = "global-allocator", global_allocator)]
pub static HEAP: EspHeap = EspHeap::empty();

#[cfg(feature = "alloc-hooks")]
unsafe extern "Rust" {
    fn _esp_alloc_alloc(heap: &EspHeap, caps: EnumSet<MemoryCapability>, ptr: usize, size: usize);
    fn _esp_alloc_dealloc(heap: &EspHeap, ptr: usize, size: usize);
}

const BAR_WIDTH: usize = 35;

fn write_bar(f: &mut core::fmt::Formatter<'_>, usage_percent: usize) -> core::fmt::Result {
    let used_blocks = BAR_WIDTH * usage_percent / 100;
    (0..used_blocks).try_for_each(|_| write!(f, "█"))?;
    (used_blocks..BAR_WIDTH).try_for_each(|_| write!(f, "░"))
}

#[cfg(feature = "defmt")]
fn write_bar_defmt(fmt: defmt::Formatter, usage_percent: usize) {
    let used_blocks = BAR_WIDTH * usage_percent / 100;
    (0..used_blocks).for_each(|_| defmt::write!(fmt, "█"));
    (used_blocks..BAR_WIDTH).for_each(|_| defmt::write!(fmt, "░"));
}

#[derive(EnumSetType, Debug)]
/// Describes the properties of a memory region
pub enum MemoryCapability {
    /// Memory must be internal; specifically it should not disappear when
    /// flash/spiram cache is switched off
    Internal,
    /// Memory must be in SPI RAM
    External,
}

/// Stats for a heap region
#[derive(Debug)]
pub struct RegionStats {
    /// Total usable size of the heap region in bytes.
    pub size: usize,

    /// Currently used size of the heap region in bytes.
    pub used: usize,

    /// Free size of the heap region in bytes.
    pub free: usize,

    /// Capabilities of the memory region.
    pub capabilities: EnumSet<MemoryCapability>,
}

impl Display for RegionStats {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let usage_percent = self.used * 100 / self.size;

        // Display Memory type
        if self.capabilities.contains(MemoryCapability::Internal) {
            write!(f, "Internal")?;
        } else if self.capabilities.contains(MemoryCapability::External) {
            write!(f, "External")?;
        } else {
            write!(f, "Unknown")?;
        }

        write!(f, " | ")?;

        write_bar(f, usage_percent)?;

        write!(
            f,
            " | Used: {}% (Used {} of {}, free: {})",
            usage_percent, self.used, self.size, self.free
        )
    }
}

#[cfg(feature = "defmt")]
#[allow(clippy::if_same_then_else)]
impl defmt::Format for RegionStats {
    fn format(&self, fmt: defmt::Formatter<'_>) {
        let usage_percent = self.used * 100 / self.size;

        if self.capabilities.contains(MemoryCapability::Internal) {
            defmt::write!(fmt, "Internal");
        } else if self.capabilities.contains(MemoryCapability::External) {
            defmt::write!(fmt, "External");
        } else {
            defmt::write!(fmt, "Unknown");
        }

        defmt::write!(fmt, " | ");

        write_bar_defmt(fmt, usage_percent);

        defmt::write!(
            fmt,
            " | Used: {}% (Used {} of {}, free: {})",
            usage_percent,
            self.used,
            self.size,
            self.free
        );
    }
}

/// A memory region to be used as heap memory
pub struct HeapRegion {
    heap: Heap,
    capabilities: EnumSet<MemoryCapability>,
}

impl HeapRegion {
    /// Create a new [HeapRegion] with the given capabilities
    ///
    /// # Safety
    ///
    /// - The supplied memory region must be available for the entire program (`'static`).
    /// - The supplied memory region must be exclusively available to the heap only, no aliasing.
    /// - `size > 0`.
    pub unsafe fn new(
        heap_bottom: *mut u8,
        size: usize,
        capabilities: EnumSet<MemoryCapability>,
    ) -> Self {
        Self {
            heap: unsafe { Heap::new(heap_bottom, size) },
            capabilities,
        }
    }

    /// Return stats for the current memory region
    pub fn stats(&self) -> RegionStats {
        RegionStats {
            size: self.size(),
            used: self.used(),
            free: self.free(),
            capabilities: self.capabilities,
        }
    }

    fn size(&self) -> usize {
        self.heap.size()
    }

    fn used(&self) -> usize {
        self.heap.used()
    }

    fn free(&self) -> usize {
        self.heap.free()
    }

    fn allocate(&mut self, layout: Layout) -> Option<NonNull<u8>> {
        self.heap.allocate(layout)
    }

    unsafe fn try_deallocate(&mut self, ptr: NonNull<u8>, layout: Layout) -> bool {
        unsafe { self.heap.try_deallocate(ptr, layout) }
    }
}

/// Stats for a heap allocator
///
/// Enable the "internal-heap-stats" feature if you want collect additional heap
/// informations at the cost of extra cpu time during every alloc/dealloc.
///
/// LP fork: the region cap below was 3 upstream — see `README-LP.md`.
#[derive(Debug)]
pub struct HeapStats {
    /// Granular stats for all the configured memory regions.
    pub region_stats: [Option<RegionStats>; MAX_REGIONS],

    /// Total size of all combined heap regions in bytes.
    pub size: usize,

    /// Current usage of the heap across all configured regions in bytes.
    pub current_usage: usize,

    /// Estimation of the max used heap in bytes.
    #[cfg(feature = "internal-heap-stats")]
    pub max_usage: usize,

    /// Estimation of the total allocated bytes since initialization.
    #[cfg(feature = "internal-heap-stats")]
    pub total_allocated: u64,

    /// Estimation of the total freed bytes since initialization.
    #[cfg(feature = "internal-heap-stats")]
    pub total_freed: u64,
}

impl Display for HeapStats {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        writeln!(f, "HEAP INFO")?;
        writeln!(f, "Size: {}", self.size)?;
        writeln!(f, "Current usage: {}", self.current_usage)?;
        #[cfg(feature = "internal-heap-stats")]
        {
            writeln!(f, "Max usage: {}", self.max_usage)?;
            writeln!(f, "Total freed: {}", self.total_freed)?;
            writeln!(f, "Total allocated: {}", self.total_allocated)?;
        }
        writeln!(f, "Memory Layout: ")?;
        for region in self.region_stats.iter() {
            if let Some(region) = region.as_ref() {
                region.fmt(f)?;
                writeln!(f)?;
            }
        }
        Ok(())
    }
}

#[cfg(feature = "defmt")]
impl defmt::Format for HeapStats {
    fn format(&self, fmt: defmt::Formatter<'_>) {
        defmt::write!(fmt, "HEAP INFO\n");
        defmt::write!(fmt, "Size: {}\n", self.size);
        defmt::write!(fmt, "Current usage: {}\n", self.current_usage);
        #[cfg(feature = "internal-heap-stats")]
        {
            defmt::write!(fmt, "Max usage: {}\n", self.max_usage);
            defmt::write!(fmt, "Total freed: {}\n", self.total_freed);
            defmt::write!(fmt, "Total allocated: {}\n", self.total_allocated);
        }
        defmt::write!(fmt, "Memory Layout:\n");
        for region in self.region_stats.iter() {
            if let Some(region) = region.as_ref() {
                defmt::write!(fmt, "{}\n", region);
            }
        }
    }
}

/// Internal stats to keep track across multiple regions.
#[cfg(feature = "internal-heap-stats")]
struct InternalHeapStats {
    max_usage: usize,
    total_allocated: u64,
    total_freed: u64,
}

struct EspHeapInner {
    heap: [Option<HeapRegion>; MAX_REGIONS],
    #[cfg(feature = "internal-heap-stats")]
    internal_heap_stats: InternalHeapStats,
    #[cfg(feature = "lend-region")]
    lend: LendState,
}

/// ⚠️ **LP fork, RESEARCH (E11, `research/ram-e11`; never for main as
/// written).** One region is the big block's: the general heap reaches it
/// LAST (only when every other region is out), and a borrower's
/// allocations reach it FIRST. Who is a borrower is the firmware's call,
/// asked on every allocation through `_esp_alloc_lend_borrower`, from inside
/// the heap's lock: it must be a few atomic loads, never an allocation.
#[cfg(feature = "lend-region")]
#[derive(Clone, Copy, Debug, Default)]
pub struct LendStats {
    /// Borrower bytes that did not fit in the block and went elsewhere.
    pub overflow_bytes: u32,
    /// How many borrower allocations did that.
    pub overflow_count: u32,
    /// Non-borrower bytes that landed in the block (every other region full).
    pub spill_bytes: u32,
    /// How many non-borrower allocations did that.
    pub spill_count: u32,
    /// Borrower allocations served from the block.
    pub borrower_count: u32,
    /// The block's peak used bytes since the last [`EspHeap::lend_reset_peak`].
    pub peak_used: u32,
}

#[cfg(feature = "lend-region")]
struct LendState {
    region: Option<usize>,
    stats: LendStats,
}

#[cfg(feature = "lend-region")]
unsafe extern "Rust" {
    /// Whether the allocation being made now is the borrower's. Called with
    /// the heap's lock held: no allocation, no lock, no logging.
    fn _esp_alloc_lend_borrower() -> bool;
}

impl EspHeapInner {
    /// Crate a new UNINITIALIZED heap allocator
    pub const fn empty() -> Self {
        EspHeapInner {
            heap: [const { None }; MAX_REGIONS],
            #[cfg(feature = "internal-heap-stats")]
            internal_heap_stats: InternalHeapStats {
                max_usage: 0,
                total_allocated: 0,
                total_freed: 0,
            },
            #[cfg(feature = "lend-region")]
            lend: LendState {
                region: None,
                stats: LendStats {
                    overflow_bytes: 0,
                    overflow_count: 0,
                    spill_bytes: 0,
                    spill_count: 0,
                    borrower_count: 0,
                    peak_used: 0,
                },
            },
        }
    }

    /// The lend-region allocation path (see [`LendStats`]).
    #[cfg(feature = "lend-region")]
    fn allocate_lend(
        &mut self,
        lend: usize,
        capabilities: EnumSet<MemoryCapability>,
        layout: Layout,
    ) -> Option<NonNull<u8>> {
        let lend_matches = self.heap[lend]
            .as_ref()
            .is_some_and(|region| region.capabilities.is_superset(capabilities));
        // SAFETY: the firmware's predicate takes no lock and allocates nothing.
        let borrower = lend_matches && unsafe { _esp_alloc_lend_borrower() };
        let size = layout.size() as u32;
        if borrower {
            if let Some(allocation) = self.heap[lend].as_mut().and_then(|r| r.allocate(layout)) {
                self.lend.stats.borrower_count += 1;
                self.note_lend_peak(lend);
                return Some(allocation);
            }
        }
        for (index, region) in self.heap.iter_mut().enumerate() {
            if index == lend {
                continue;
            }
            let Some(region) = region.as_mut() else {
                continue;
            };
            if !region.capabilities.is_superset(capabilities) {
                continue;
            }
            if let Some(allocation) = region.allocate(layout) {
                if borrower {
                    self.lend.stats.overflow_bytes = self.lend.stats.overflow_bytes.saturating_add(size);
                    self.lend.stats.overflow_count += 1;
                }
                return Some(allocation);
            }
        }
        if lend_matches && !borrower {
            if let Some(allocation) = self.heap[lend].as_mut().and_then(|r| r.allocate(layout)) {
                self.lend.stats.spill_bytes = self.lend.stats.spill_bytes.saturating_add(size);
                self.lend.stats.spill_count += 1;
                self.note_lend_peak(lend);
                return Some(allocation);
            }
        }
        None
    }

    #[cfg(feature = "lend-region")]
    fn note_lend_peak(&mut self, lend: usize) {
        if let Some(region) = self.heap[lend].as_ref() {
            let used = region.used() as u32;
            if used > self.lend.stats.peak_used {
                self.lend.stats.peak_used = used;
            }
        }
    }

    /// The lend region's longest free run, by binary search with real
    /// allocations in that region alone (the lock is held throughout, so
    /// nothing else sees the probes).
    #[cfg(feature = "lend-region")]
    fn lend_largest_free(&mut self) -> usize {
        let Some(lend) = self.lend.region else {
            return 0;
        };
        let Some(region) = self.heap[lend].as_mut() else {
            return 0;
        };
        let (mut low, mut high) = (0usize, region.free());
        while low < high {
            let mid = (low + high + 1) / 2;
            let Ok(layout) = Layout::from_size_align(mid.max(1), 4) else {
                break;
            };
            match region.allocate(layout) {
                Some(ptr) => {
                    // SAFETY: just allocated in this region with this layout.
                    unsafe { region.try_deallocate(ptr, layout) };
                    low = mid;
                }
                None => high = mid - 1,
            }
        }
        low
    }

    pub unsafe fn add_region(&mut self, region: HeapRegion) {
        let free = self
            .heap
            .iter()
            .enumerate()
            .find(|v| v.1.is_none())
            .map(|v| v.0);

        if let Some(free) = free {
            self.heap[free] = Some(region);
        } else {
            panic!(
                "Exceeded the maximum of {} heap memory regions",
                self.heap.len()
            );
        }
    }

    /// Returns an estimate of the amount of bytes in use in all memory regions.
    pub fn used(&self) -> usize {
        let mut used = 0;
        for region in self.heap.iter() {
            if let Some(region) = region.as_ref() {
                used += region.heap.used();
            }
        }
        used
    }

    /// Return usage stats for the [EspHeap].
    ///
    /// Note:
    /// [HeapStats] directly implements [Display], so this function can be
    /// called from within `println!()` to pretty-print the usage of the
    /// heap.
    pub fn stats(&self) -> HeapStats {
        let mut region_stats: [Option<RegionStats>; MAX_REGIONS] = [const { None }; MAX_REGIONS];

        let mut used = 0;
        let mut free = 0;
        for (id, region) in self.heap.iter().enumerate() {
            if let Some(region) = region.as_ref() {
                let stats = region.stats();
                free += stats.free;
                used += stats.used;
                region_stats[id] = Some(region.stats());
            }
        }

        cfg_if::cfg_if! {
            if #[cfg(feature = "internal-heap-stats")] {
                HeapStats {
                    region_stats,
                    size: free + used,
                    current_usage: used,
                    max_usage: self.internal_heap_stats.max_usage,
                    total_allocated: self.internal_heap_stats.total_allocated,
                    total_freed: self.internal_heap_stats.total_freed,
                }
            } else {
                HeapStats {
                    region_stats,
                    size: free + used,
                    current_usage: used,
                }
            }
        }
    }

    /// Returns an estimate of the amount of bytes available.
    pub fn free(&self) -> usize {
        self.free_caps(EnumSet::empty())
    }

    /// The free heap satisfying the given requirements
    pub fn free_caps(&self, capabilities: EnumSet<MemoryCapability>) -> usize {
        let mut free = 0;
        for region in self.heap.iter().filter(|region| {
            if region.is_some() {
                region
                    .as_ref()
                    .unwrap()
                    .capabilities
                    .is_superset(capabilities)
            } else {
                false
            }
        }) {
            if let Some(region) = region.as_ref() {
                free += region.heap.free();
            }
        }
        free
    }

    /// Allocate memory in a region satisfying the given requirements.
    ///
    /// # Safety
    ///
    /// This function is unsafe because undefined behavior can result
    /// if the caller does not ensure that `layout` has non-zero size.
    ///
    /// The allocated block of memory may or may not be initialized.
    unsafe fn alloc_caps(
        &mut self,
        capabilities: EnumSet<MemoryCapability>,
        layout: Layout,
    ) -> *mut u8 {
        #[cfg(feature = "internal-heap-stats")]
        let before = self.used();
        #[cfg(feature = "lend-region")]
        let lent = self
            .lend
            .region
            .map(|lend| self.allocate_lend(lend, capabilities, layout));
        #[cfg(not(feature = "lend-region"))]
        let lent: Option<Option<NonNull<u8>>> = None;
        let allocation = if let Some(lent) = lent {
            let Some(allocation) = lent else {
                return ptr::null_mut();
            };
            allocation
        } else {
            let mut iter = self
                .heap
                .iter_mut()
                .filter_map(|region| region.as_mut())
                .filter(|region| region.capabilities.is_superset(capabilities));

            loop {
                let Some(region) = iter.next() else {
                    return ptr::null_mut();
                };

                if let Some(res) = region.allocate(layout) {
                    break res;
                }
            }
        };

        #[cfg(feature = "internal-heap-stats")]
        {
            // We need to call used because the heap impls have some internal overhead
            // so we cannot use the size provided by the layout.
            let used = self.used();

            self.internal_heap_stats.total_allocated = self
                .internal_heap_stats
                .total_allocated
                .saturating_add((used - before) as u64);
            self.internal_heap_stats.max_usage =
                core::cmp::max(self.internal_heap_stats.max_usage, used);
        }

        allocation.as_ptr()
    }
}

/// A memory allocator
///
/// In addition to what Rust's memory allocator can do it allows to allocate
/// memory in regions satisfying specific needs.
pub struct EspHeap {
    inner: NonReentrantMutex<EspHeapInner>,
}

impl EspHeap {
    /// Crate a new UNINITIALIZED heap allocator
    pub const fn empty() -> Self {
        EspHeap {
            inner: NonReentrantMutex::new(EspHeapInner::empty()),
        }
    }

    /// Add a memory region to the heap
    ///
    /// `heap_bottom` is a pointer to the location of the bottom of the heap.
    ///
    /// `size` is the size of the heap in bytes.
    ///
    /// You can add up to [`MAX_REGIONS`] regions per allocator.
    ///
    /// Note that:
    ///
    /// - Memory is allocated from the first suitable memory region first
    ///
    /// - The heap grows "upwards", towards larger addresses. Thus `end_addr` must be larger than
    ///   `start_addr`
    ///
    /// - The size of the heap is `(end_addr as usize) - (start_addr as usize)`. The allocator won't
    ///   use the byte at `end_addr`.
    ///
    /// # Safety
    ///
    /// - The supplied memory region must be available for the entire program (a `'static`
    ///   lifetime).
    /// - The supplied memory region must be exclusively available to the heap only, no aliasing.
    /// - `size > 0`.
    pub unsafe fn add_region(&self, region: HeapRegion) {
        self.inner.with(|heap| unsafe { heap.add_region(region) })
    }

    /// Returns an estimate of the amount of bytes in use in all memory regions.
    pub fn used(&self) -> usize {
        self.inner.with(|heap| heap.used())
    }

    /// RESEARCH (E11): make the region registered `index`-th (0-based, in
    /// [`Self::add_region`] order) the lend region. See [`LendStats`].
    #[cfg(feature = "lend-region")]
    pub fn set_lend_region(&self, index: usize) {
        self.inner.with(|heap| heap.lend.region = Some(index));
    }

    /// RESEARCH (E11): the lend region's counters.
    #[cfg(feature = "lend-region")]
    pub fn lend_stats(&self) -> LendStats {
        self.inner.with(|heap| heap.lend.stats)
    }

    /// RESEARCH (E11): the lend region's `(used, free)` bytes.
    #[cfg(feature = "lend-region")]
    pub fn lend_used_free(&self) -> (usize, usize) {
        self.inner.with(|heap| {
            heap.lend
                .region
                .and_then(|index| heap.heap[index].as_ref())
                .map_or((0, 0), |region| (region.used(), region.free()))
        })
    }

    /// RESEARCH (E11): restart the lend region's peak from its use now.
    #[cfg(feature = "lend-region")]
    pub fn lend_reset_peak(&self) {
        self.inner.with(|heap| {
            let used = heap
                .lend
                .region
                .and_then(|index| heap.heap[index].as_ref())
                .map_or(0, |region| region.used() as u32);
            heap.lend.stats.peak_used = used;
        })
    }

    /// RESEARCH (E11): the lend region's longest free run.
    #[cfg(feature = "lend-region")]
    pub fn lend_largest_free(&self) -> usize {
        self.inner.with(|heap| heap.lend_largest_free())
    }

    /// Return usage stats for the [EspHeap].
    ///
    /// Note:
    /// [HeapStats] directly implements [Display], so this function can be
    /// called from within `println!()` to pretty-print the usage of the
    /// heap.
    pub fn stats(&self) -> HeapStats {
        self.inner.with(|heap| heap.stats())
    }

    /// Returns an estimate of the amount of bytes available.
    pub fn free(&self) -> usize {
        self.inner.with(|heap| heap.free())
    }

    /// The free heap satisfying the given requirements
    pub fn free_caps(&self, capabilities: EnumSet<MemoryCapability>) -> usize {
        self.inner.with(|heap| heap.free_caps(capabilities))
    }

    /// Allocate memory in a region satisfying the given requirements.
    ///
    /// # Safety
    ///
    /// This function is unsafe because undefined behavior can result
    /// if the caller does not ensure that `layout` has non-zero size.
    ///
    /// The allocated block of memory may or may not be initialized.
    pub unsafe fn alloc_caps(
        &self,
        capabilities: EnumSet<MemoryCapability>,
        layout: Layout,
    ) -> *mut u8 {
        let ptr = self
            .inner
            .with(|heap| unsafe { heap.alloc_caps(capabilities, layout) });

        #[cfg(feature = "alloc-hooks")]
        unsafe {
            _esp_alloc_alloc(self, capabilities, ptr.addr(), layout.size());
        }

        ptr
    }

    /// Deallocate memory.
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        #[cfg(feature = "alloc-hooks")]
        unsafe {
            _esp_alloc_dealloc(self, ptr.addr(), layout.size());
        }

        let Some(ptr) = NonNull::new(ptr) else {
            return;
        };

        self.inner.with(|this| {
            #[cfg(feature = "internal-heap-stats")]
            let before = this.used();
            let mut iter = this.heap.iter_mut();

            while let Some(Some(region)) = iter.next() {
                if unsafe { region.try_deallocate(ptr, layout) } {
                    break;
                }
            }

            #[cfg(feature = "internal-heap-stats")]
            {
                // We need to call `used()` because [linked_list_allocator::Heap] does internal
                // size alignment so we cannot use the size provided by the
                // layout.
                this.internal_heap_stats.total_freed = this
                    .internal_heap_stats
                    .total_freed
                    .saturating_add((before - this.used()) as u64);
            }
        })
    }
}

unsafe impl GlobalAlloc for EspHeap {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        unsafe { self.alloc_caps(EnumSet::empty(), layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { self.dealloc(ptr, layout) }
    }
}
