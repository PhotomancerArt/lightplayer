//! Replay a heap trace through the allocator esp-alloc reaches for when its
//! `heap_algorithm` is set to `TLSF`, so "would TLSF have kept the classic's
//! big block alive" is a measured number rather than an opinion.
//!
//! Unlike the first-fit side of this module, nothing is modelled here: the
//! real `rlsf` crate, at the version and generic instantiation esp-alloc 0.10
//! uses (`Tlsf<'static, usize, usize, 32, 32>`, inserted with
//! `insert_free_block_ptr` exactly as `esp-alloc/src/heap/tlsf.rs` does), runs
//! over real host buffers, and the free-list shape is read back with
//! `Tlsf::iter_blocks` — the same walk esp-alloc's own `free()` uses.
//!
//! ⚠️ **The geometry is the host's, not the device's.** `rlsf`'s block
//! geometry is derived from `size_of::<usize>()`: `GRANULARITY` is
//! `size_of::<usize>() * 4` and the per-block header is half of that. On the
//! 32-bit target that is a 16 B granule and an 8 B header; on this 64-bit host
//! it is 32 B and 16 B. The crate's generic parameters (`FLBitmap`, `SLBitmap`,
//! `FLLEN`, `SLLEN`) only choose the bitmap words — `usize` is hard-wired into
//! the size arithmetic — so there is no 32-bit-width instantiation to ask for
//! on this host, the way [`super::first_fit_heap`] models one for
//! `linked_list_allocator`. Every TLSF row therefore carries its header width
//! and says so: it over-charges each live block by 8 B and rounds to a 32 B
//! granule instead of 16 B, which makes it a *pessimistic* stand-in for the
//! device's TLSF, not an exact one.

use ::alloc::string::{String, ToString};
use ::alloc::vec::Vec;
use core::alloc::Layout;
use core::ptr::NonNull;
use std::collections::HashMap;

use crate::profile::alloc::TraceEventOwned;

use super::frag_discount::DiscountMatcher;
use super::frag_replay::RegionSpec;

/// `rlsf`'s allocation granularity on this host: `size_of::<usize>() * 4`.
pub const TLSF_GRANULARITY: usize = rlsf::GRANULARITY;

/// `size_of::<BlockHdr>()` — the per-block header every live block carries,
/// which is `GRANULARITY / 2` by construction (2 × `usize`).
pub const TLSF_HEADER_BYTES: usize = rlsf::GRANULARITY / 2;

/// The header width the device would use, for the drift note in the report.
pub const TLSF_DEVICE_HEADER_BYTES: usize = 8;

/// The allocation granularity the device would use: `size_of::<u32>() * 4`.
pub const TLSF_DEVICE_GRANULARITY: usize = 16;

/// What one live block of `size` bytes costs this host's TLSF, over what it
/// would cost the device's — the price of the geometry drift, per block.
///
/// It is reported per marker rather than argued about: on a workload of
/// thousands of small allocations, a 16 B header rounded to a 32 B granule
/// instead of an 8 B header rounded to 16 B is not a rounding error, it is the
/// difference between "TLSF would have helped" and "the pool ran out". A row
/// whose surcharge is a large fraction of the layout is measuring the host,
/// not the allocator.
fn geometry_surcharge(size: u32) -> u32 {
    let host = (TLSF_HEADER_BYTES + size as usize).next_multiple_of(TLSF_GRANULARITY);
    let device =
        (TLSF_DEVICE_HEADER_BYTES + size as usize).next_multiple_of(TLSF_DEVICE_GRANULARITY);
    u32::try_from(host.saturating_sub(device)).unwrap_or(u32::MAX)
}

/// The exact instantiation esp-alloc 0.10 uses for its `TLSF` heap algorithm.
type EspAllocTlsf = rlsf::Tlsf<'static, usize, usize, 32, 32>;

/// The free space in one TLSF pool at one perf marker, measured the way
/// esp-alloc measures it: by walking the pool's blocks and taking the
/// unoccupied ones' payload capacity.
#[derive(Debug, Clone)]
pub struct TlsfMarkerShape {
    pub name: String,
    pub kind: String,
    pub ic: u64,
    pub holes: u32,
    pub largest: u32,
    pub free: u32,
    /// Largest free payload per region, in registration order.
    pub region_largest: Vec<u32>,
    /// Bytes the live set costs on this host's TLSF geometry over what it
    /// would cost the device's — see [`geometry_surcharge`].
    pub geometry_surcharge: u32,
}

/// Replay `events` through one TLSF pool per region and snapshot the free
/// space at every `"t":"P"` marker row.
///
/// `discounts` drops the same call sites the first-fit replay drops, so the
/// two tables answer the same question about the same workload.
pub fn replay_tlsf(
    events: &[TraceEventOwned],
    regions: &[RegionSpec],
    discounts: &mut DiscountMatcher<'_>,
) -> TlsfReplayResult {
    let mut heaps: Vec<TlsfPool> = regions.iter().map(|r| TlsfPool::new(r.size)).collect();
    let mut live: HashMap<u32, LiveBlock> = HashMap::new();
    let mut markers = Vec::new();
    let mut would_oom = 0u64;

    let mut allocate = |heaps: &mut Vec<TlsfPool>,
                        live: &mut HashMap<u32, LiveBlock>,
                        event: &TraceEventOwned,
                        would_oom: &mut u64| {
        if discounts.matches(&event.frames).is_some() {
            return;
        }
        let align = if event.align.is_power_of_two() {
            event.align
        } else {
            crate::profile::alloc::DEFAULT_TRACE_ALIGN
        };
        // `Layout` refuses a zero size; the guest's allocator raises such a
        // request to its minimum block, and so does this one.
        let size = event.sz.max(1) as usize;
        let Ok(layout) = Layout::from_size_align(size, align as usize) else {
            return;
        };
        for (index, pool) in heaps.iter_mut().enumerate() {
            if let Some(ptr) = pool.allocate(layout) {
                live.insert(
                    event.ptr,
                    LiveBlock {
                        region: index,
                        ptr,
                        align: align as usize,
                        surcharge: geometry_surcharge(event.sz),
                    },
                );
                return;
            }
        }
        *would_oom += 1;
    };

    for event in events {
        match event.t.as_str() {
            "A" => allocate(&mut heaps, &mut live, event, &mut would_oom),
            "D" => deallocate(&mut heaps, &mut live, event.ptr),
            "R" => {
                // Same order as the guest's `TrackingAllocator::realloc` and
                // the first-fit replay: the new block is placed before the old
                // one is released, so it cannot land in the old one's space.
                allocate(&mut heaps, &mut live, event, &mut would_oom);
                deallocate(&mut heaps, &mut live, event.old_ptr.unwrap_or(0));
            }
            "P" => {
                let mut shape = TlsfMarkerShape {
                    name: event.name.clone().unwrap_or_else(|| "?".to_string()),
                    kind: event.kind.clone().unwrap_or_else(|| "?".to_string()),
                    ic: event.ic,
                    holes: 0,
                    largest: 0,
                    free: 0,
                    region_largest: Vec::with_capacity(heaps.len()),
                    geometry_surcharge: live.values().map(|b| b.surcharge).sum(),
                };
                for pool in &heaps {
                    let (holes, largest, free) = pool.free_shape();
                    shape.holes += holes;
                    shape.free += free;
                    shape.largest = shape.largest.max(largest);
                    shape.region_largest.push(largest);
                }
                markers.push(shape);
            }
            _ => {}
        }
    }

    TlsfReplayResult {
        markers,
        would_oom,
        header_bytes: TLSF_HEADER_BYTES,
        granularity: TLSF_GRANULARITY,
    }
}

/// Everything one TLSF replay produced.
#[derive(Debug)]
pub struct TlsfReplayResult {
    pub markers: Vec<TlsfMarkerShape>,
    /// Requests no region could serve. They are skipped, so every figure after
    /// the first one is optimistic — the same rule the first-fit replay uses.
    pub would_oom: u64,
    pub header_bytes: usize,
    pub granularity: usize,
}

impl TlsfReplayResult {
    /// The largest geometry surcharge any marker carried — the headline number
    /// for "how much of this row is the host's word size".
    pub fn peak_geometry_surcharge(&self) -> u32 {
        self.markers
            .iter()
            .map(|m| m.geometry_surcharge)
            .max()
            .unwrap_or(0)
    }
}

/// One live block, keyed by the guest pointer the trace used.
struct LiveBlock {
    region: usize,
    ptr: NonNull<u8>,
    /// `rlsf::Tlsf::deallocate` needs the alignment the allocation was made
    /// with, so it is carried rather than re-derived.
    align: usize,
    /// This block's share of [`geometry_surcharge`], summed at every marker.
    surcharge: u32,
}

/// One `rlsf` pool over a leaked host buffer.
///
/// The buffer is leaked because `Tlsf<'static, …>` — the instantiation
/// esp-alloc uses — requires the pool to outlive the allocator, and a replay
/// pool is a few hundred kilobytes held for the length of one CLI run.
struct TlsfPool {
    tlsf: EspAllocTlsf,
    /// The pool slice as `insert_free_block_ptr` accepted it, i.e. trimmed to
    /// the length it reported back. `iter_blocks` requires exactly this.
    pool: NonNull<[u8]>,
}

impl TlsfPool {
    /// A pool of `size` bytes, base-aligned to [`POOL_ALIGN`] so that
    /// placement depends only on the pool-relative offsets — two runs of the
    /// same trace must produce the same table, and an arbitrary malloc address
    /// would make requests with a large `align` land differently each time.
    fn new(size: u32) -> Self {
        const POOL_ALIGN: usize = 4096;
        let layout = Layout::from_size_align(size as usize, POOL_ALIGN)
            .expect("region size and a 4 KiB alignment are a valid layout");
        // SAFETY: `layout` has a non-zero size (every modelled region does),
        // and the allocation is leaked, so the buffer outlives the `Tlsf` that
        // borrows it for `'static`.
        let raw = unsafe { ::alloc::alloc::alloc(layout) };
        assert!(
            !raw.is_null(),
            "out of memory allocating a TLSF replay pool"
        );
        let block = NonNull::slice_from_raw_parts(
            NonNull::new(raw).expect("just checked for null"),
            size as usize,
        );

        let mut tlsf = EspAllocTlsf::new();
        // SAFETY: `block` is a uniquely-owned, leaked buffer that nothing else
        // reads or writes, which is exactly what `insert_free_block_ptr`
        // requires. Mirrors `esp-alloc/src/heap/tlsf.rs::TlsfHeap::new`.
        let actual = unsafe { tlsf.insert_free_block_ptr(block) }
            .expect("a modelled region is far larger than TLSF's minimum pool");
        let pool = NonNull::slice_from_raw_parts(
            NonNull::new(raw).expect("just checked for null"),
            actual.get(),
        );
        Self { tlsf, pool }
    }

    fn allocate(&mut self, layout: Layout) -> Option<NonNull<u8>> {
        self.tlsf.allocate(layout)
    }

    /// # Safety
    ///
    /// `ptr` must have come from this pool's [`Self::allocate`] with `align`,
    /// and must not have been freed since.
    unsafe fn deallocate(&mut self, ptr: NonNull<u8>, align: usize) {
        unsafe { self.tlsf.deallocate(ptr, align) };
    }

    /// `(holes, largest, free)` over the pool's unoccupied blocks, in payload
    /// bytes — the same measurement `esp-alloc`'s `TlsfHeap::free` reports and
    /// the same one the device's heartbeat carries.
    fn free_shape(&self) -> (u32, u32, u32) {
        let mut holes = 0u32;
        let mut largest = 0u32;
        let mut free = 0u32;
        // SAFETY: `self.pool` is the slice `insert_free_block_ptr` accepted,
        // trimmed to the length it returned, and this pool's blocks are only
        // mutated through `&mut self` methods — which cannot run concurrently
        // with this `&self` walk.
        for block in unsafe { self.tlsf.iter_blocks(self.pool) } {
            if block.is_occupied() {
                continue;
            }
            let payload = block.max_payload_size() as u32;
            holes += 1;
            free += payload;
            largest = largest.max(payload);
        }
        (holes, largest, free)
    }
}

fn deallocate(heaps: &mut [TlsfPool], live: &mut HashMap<u32, LiveBlock>, ptr: u32) {
    let Some(block) = live.remove(&ptr) else {
        return;
    };
    // SAFETY: `block` was produced by `heaps[block.region].allocate` with
    // `block.align` and has just been removed from the live map, so this is
    // its only free.
    unsafe { heaps[block.region].deallocate(block.ptr, block.align) };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_trivial_trace_places_blocks_and_reports_holes_with_the_header() {
        // Three blocks placed back to back, the middle one freed: the pool
        // must report the freed block's hole plus the pool's tail, and the
        // hole must be the payload the middle block occupied — its header
        // included in the block, excluded from the payload.
        let regions = [RegionSpec {
            index: 0,
            base: 0,
            size: 4096,
        }];
        let resolver = crate::profile::alloc::SymbolResolver::empty();
        let mut discounts = DiscountMatcher::new(&resolver, &[]);
        let events = [
            row_alloc(1, 64, 10),
            row_alloc(2, 64, 20),
            row_alloc(3, 64, 30),
            row_free(2, 40),
            row_marker("probe", 50),
        ];

        let result = replay_tlsf(&events, &regions, &mut discounts);
        let shape = result.markers.last().expect("one marker");

        assert_eq!(result.would_oom, 0);
        assert_eq!(shape.holes, 2, "the freed block's hole plus the pool tail");
        assert_eq!(
            result.header_bytes,
            TLSF_GRANULARITY / 2,
            "the header is half a granule by construction"
        );
        // The middle block's whole footprint is `header + payload` rounded up
        // to a granule; freeing it returns a block of that size, whose payload
        // capacity is one header less.
        let footprint = (TLSF_HEADER_BYTES + 64).next_multiple_of(TLSF_GRANULARITY);
        let expected_hole = (footprint - TLSF_HEADER_BYTES) as u32;
        assert!(
            shape.region_largest[0] > expected_hole,
            "the tail is the largest hole, not the freed 64 B block"
        );
        assert_eq!(
            shape.free,
            shape.region_largest[0] + expected_hole,
            "free total is the tail plus the freed block's payload capacity"
        );
    }

    #[test]
    fn a_request_no_region_can_serve_is_counted_not_placed() {
        let regions = [RegionSpec {
            index: 0,
            base: 0,
            size: 4096,
        }];
        let resolver = crate::profile::alloc::SymbolResolver::empty();
        let mut discounts = DiscountMatcher::new(&resolver, &[]);
        let events = [row_alloc(1, 100_000, 10), row_marker("probe", 20)];

        let result = replay_tlsf(&events, &regions, &mut discounts);
        assert_eq!(result.would_oom, 1);
    }

    fn row_alloc(ptr: u32, sz: u32, ic: u64) -> TraceEventOwned {
        TraceEventOwned::synthetic_alloc(ptr, sz, 4, Vec::new(), ic)
    }

    fn row_free(ptr: u32, ic: u64) -> TraceEventOwned {
        TraceEventOwned::synthetic_free(ptr, ic)
    }

    fn row_marker(name: &str, ic: u64) -> TraceEventOwned {
        TraceEventOwned::synthetic_marker(name.to_string(), "I".to_string(), ic)
    }
}
