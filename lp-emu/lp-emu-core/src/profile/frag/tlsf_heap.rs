//! Replay a heap trace through the allocator esp-alloc reaches for when its
//! `heap_algorithm` is set to `TLSF`, at the DEVICE's geometry, so "would TLSF
//! have kept the big block alive" is a measured number rather than an opinion.
//!
//! esp-alloc 0.10 builds `rlsf::Tlsf<'static, usize, usize, 32, 32>` (rlsf
//! 0.2.2), and rlsf derives its block geometry from `size_of::<usize>()`:
//! `GRANULARITY` is four words and a block's header two. The real crate on
//! this 64-bit host would therefore replay a 32 B granule and a 16 B header,
//! where the 32-bit devices (the C6, the classic, the S3) have 16 B and 8 B —
//! which is what this module's first version did, and why its rows were a
//! pessimistic bound and nothing more.
//!
//! [`TlsfModel`] is an independent model of rlsf 0.2.2's `Tlsf` at any word
//! width, written from reading its `src/tlsf.rs` (crates.io, MIT OR
//! Apache-2.0, by yvt; no code copied): segregated free lists `FLLEN × 32`
//! with first- and second-level bitmaps, good-fit search (`map_ceil` rounds a
//! request up to the next list boundary, the head of the first non-empty list
//! at or above it is taken), split with the remainder pushed onto its list's
//! head, LIFO frees with immediate coalescing of both physical neighbours,
//! and a 16-aligned pool capped by a one-granule sentinel. Its tests check it
//! address for address against the real crate at this host's width (`W = 8`),
//! the one width the real crate can run at here; the replay uses `W = 4`. The
//! same model, in Python (`scripts/ram/tlsf_model.py`), reproduced every
//! placement of a TLSF C6 image's own allocation trace (RAM research E6).
//!
//! Two figures per region, because they differ under TLSF: the largest free
//! block's payload (what esp-alloc's `free()` walk sees) and the largest
//! request it would serve (what a probe like the firmware's
//! `largest_free_block()` finds): good fit never searches the list a request's
//! own size falls in, so the second is `list_min(largest block's list) − header`.
//! The counterfactual cell reports the second.

use ::alloc::string::{String, ToString};
use ::alloc::vec::Vec;
use std::collections::{BTreeMap, HashMap};

use crate::profile::alloc::TraceEventOwned;

use super::frag_discount::DiscountMatcher;
use super::frag_replay::RegionSpec;

/// The word width of every device this replays for (rv32, Xtensa).
pub const DEVICE_WORD: usize = 4;

/// `rlsf::GRANULARITY` on the device: `size_of::<usize>() * 4`.
pub const TLSF_GRANULARITY: usize = DEVICE_WORD * 4;

/// `size_of::<BlockHdr>()` on the device: the per-block cost of a used block.
pub const TLSF_HEADER_BYTES: usize = DEVICE_WORD * 2;

/// esp-alloc 0.10's first-level count (`usize::BITS`).
pub const ESP_ALLOC_FLLEN: usize = 32;

/// `rlsf`'s second-level count in esp-alloc's instantiation, and its log2.
const SLLEN: usize = 32;
const SLI: u32 = 5;

/// The free space in one TLSF pool at one perf marker.
#[derive(Debug, Clone)]
pub struct TlsfMarkerShape {
    pub name: String,
    pub kind: String,
    pub ic: u64,
    pub holes: u32,
    /// The largest free block's payload capacity, over every region.
    pub largest: u32,
    /// The largest request any region would serve (see the module docs).
    pub largest_request: u32,
    pub free: u32,
    /// Largest request per region, in registration order.
    pub region_largest: Vec<u32>,
}

/// Replay `events` through one TLSF pool per region (device geometry) and
/// snapshot the free space at every `"t":"P"` marker row.
///
/// `discounts` drops the same call sites the first-fit replay drops, so the
/// two tables answer the same question about the same workload.
pub fn replay_tlsf(
    events: &[TraceEventOwned],
    regions: &[RegionSpec],
    discounts: &mut DiscountMatcher<'_>,
) -> TlsfReplayResult {
    let mut heaps: Vec<TlsfModel> = regions
        .iter()
        .map(|r| TlsfModel::new(DEVICE_WORD, ESP_ALLOC_FLLEN, 0, r.size as u64))
        .collect();
    let mut live: HashMap<u32, (usize, u64)> = HashMap::new();
    let mut markers = Vec::new();
    let mut would_oom = 0u64;

    let mut allocate = |heaps: &mut Vec<TlsfModel>,
                        live: &mut HashMap<u32, (usize, u64)>,
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
        // The guest's allocator raises a zero size to its minimum block, and
        // so does `Layout`-taking rlsf (a zero-size request still costs one).
        let size = event.sz.max(1) as u64;
        for (index, pool) in heaps.iter_mut().enumerate() {
            if let Some(ptr) = pool.allocate(size, align as u64) {
                live.insert(event.ptr, (index, ptr));
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
                // esp-alloc's `GlobalAlloc` has no `realloc` of its own: the
                // default allocates the new block, copies, and frees the old
                // one, in that order — so the new block cannot land in the
                // old one's space. Same order as the first-fit replay.
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
                    largest_request: 0,
                    free: 0,
                    region_largest: Vec::with_capacity(heaps.len()),
                };
                for pool in &heaps {
                    let (holes, largest, free) = pool.free_shape();
                    let request = pool.largest_request() as u32;
                    shape.holes += holes;
                    shape.free += free as u32;
                    shape.largest = shape.largest.max(largest as u32);
                    shape.largest_request = shape.largest_request.max(request);
                    shape.region_largest.push(request);
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

fn deallocate(heaps: &mut [TlsfModel], live: &mut HashMap<u32, (usize, u64)>, ptr: u32) {
    if let Some((region, at)) = live.remove(&ptr) {
        heaps[region].deallocate(at);
    }
}

/// rlsf 0.2.2's `Tlsf<'_, usize, usize, FLLEN, 32>` over one pool, at a word
/// width of `word` bytes, on pool addresses (`u64`, so `W = 8` fits too).
pub struct TlsfModel {
    gran: u64,
    hdr: u64,
    gran_log2: u32,
    fllen: usize,
    /// Every block, keyed by its start: `(size including header, used)`.
    /// Sentinels are used blocks of one granule.
    blocks: BTreeMap<u64, (u64, bool)>,
    /// `prev_phys_block` of every block.
    prev: HashMap<u64, Option<u64>>,
    /// Free-list links of every free block: `(next_free, prev_free)`.
    links: HashMap<u64, (Option<u64>, Option<u64>)>,
    heads: HashMap<(usize, usize), u64>,
    fl_bitmap: u64,
    sl_bitmap: Vec<u64>,
    /// Payload pointer → block start.
    block_of: HashMap<u64, u64>,
}

impl TlsfModel {
    /// A pool over `[base, base + size)`, inserted the way
    /// `Tlsf::insert_free_block_ptr` inserts one.
    pub fn new(word: usize, fllen: usize, base: u64, size: u64) -> Self {
        let gran = (word * 4) as u64;
        let gran_log2 = gran.trailing_zeros();
        let mut model = Self {
            gran,
            hdr: (word * 2) as u64,
            gran_log2,
            fllen,
            blocks: BTreeMap::new(),
            prev: HashMap::new(),
            links: HashMap::new(),
            heads: HashMap::new(),
            fl_bitmap: 0,
            sl_bitmap: ::alloc::vec![0; fllen],
            block_of: HashMap::new(),
        };
        let start = base.next_multiple_of(gran);
        let Some(len) = size.checked_sub(start - base) else {
            return model;
        };
        let mut left = len & !(gran - 1);
        let usize_bits = (word * 8) as u32;
        let shift = gran_log2 + fllen as u32;
        let max_pool = (shift < usize_bits).then(|| 1u64 << shift);
        let mut cursor = start;
        while left >= gran * 2 {
            let chunk = max_pool.map_or(left, |m| left.min(m));
            model.blocks.insert(cursor, (chunk - gran, false));
            model.prev.insert(cursor, None);
            let sentinel = cursor + chunk - gran;
            model.blocks.insert(sentinel, (gran, true));
            model.prev.insert(sentinel, Some(cursor));
            model.link(cursor, chunk - gran);
            left -= chunk;
            cursor += chunk;
        }
        model
    }

    /// `(fl, sl)` of the list a free block of `size` bytes lives on.
    fn map_floor(&self, size: u64) -> (usize, usize) {
        let top = 63 - size.leading_zeros();
        let fl = top - self.gran_log2;
        let sl = if top >= SLI {
            (size >> (top - SLI)) as usize
        } else {
            (size << (SLI - top)) as usize
        };
        (fl as usize, sl & (SLLEN - 1))
    }

    /// `(fl, sl)` of the first list every block of which holds `size`.
    fn map_ceil(&self, size: u64) -> (usize, usize) {
        let top = 63 - size.leading_zeros();
        let mut size = size;
        if top > SLI {
            let shift = top - SLI;
            if size & ((1 << shift) - 1) != 0 {
                size = ((size >> shift) + 1) << shift;
            }
        }
        self.map_floor(size)
    }

    /// The smallest block list `(fl, sl)` holds.
    fn list_min(&self, fl: usize, sl: usize) -> u64 {
        let top = fl as u32 + self.gran_log2;
        let base = 1u64 << top;
        if top >= SLI {
            base + ((sl as u64) << (top - SLI))
        } else {
            base + ((sl as u64) >> (SLI - top))
        }
    }

    fn link(&mut self, block: u64, size: u64) {
        let (fl, sl) = self.map_floor(size);
        let old = self.heads.insert((fl, sl), block);
        self.links.insert(block, (old, None));
        if let Some(old) = old {
            self.links.get_mut(&old).expect("a linked block").1 = Some(block);
        }
        self.fl_bitmap |= 1 << fl;
        self.sl_bitmap[fl] |= 1 << sl;
    }

    fn unlink(&mut self, block: u64, size: u64) {
        let (next, prev) = self.links.remove(&block).expect("a free block is linked");
        if let Some(next) = next {
            self.links.get_mut(&next).expect("a linked block").1 = prev;
        }
        match prev {
            Some(prev) => self.links.get_mut(&prev).expect("a linked block").0 = next,
            None => {
                let (fl, sl) = self.map_floor(size);
                match next {
                    Some(next) => {
                        self.heads.insert((fl, sl), next);
                    }
                    None => {
                        self.heads.remove(&(fl, sl));
                        self.sl_bitmap[fl] &= !(1 << sl);
                        if self.sl_bitmap[fl] == 0 {
                            self.fl_bitmap &= !(1 << fl);
                        }
                    }
                }
            }
        }
    }

    fn search(&self, size: u64) -> Option<(usize, usize)> {
        let (fl, sl) = self.map_ceil(size);
        if fl >= self.fllen {
            return None;
        }
        let in_fl = self.sl_bitmap[fl] >> sl;
        if in_fl != 0 {
            return Some((fl, sl + in_fl.trailing_zeros() as usize));
        }
        let above = self.fl_bitmap.checked_shr(fl as u32 + 1).unwrap_or(0);
        if above == 0 {
            return None;
        }
        let fl = fl + 1 + above.trailing_zeros() as usize;
        if fl >= self.fllen {
            return None;
        }
        Some((fl, self.sl_bitmap[fl].trailing_zeros() as usize))
    }

    /// `Tlsf::allocate(Layout { size, align })`: the payload address, or None.
    pub fn allocate(&mut self, size: u64, align: u64) -> Option<u64> {
        let max_overhead = align.saturating_sub(self.hdr) + self.hdr;
        let search = (size + max_overhead).next_multiple_of(self.gran);
        let list = self.search(search)?;
        let block = self.heads[&list];
        let (bsize, _) = self.blocks[&block];
        self.unlink(block, bsize);
        let ptr = (block + self.hdr).next_multiple_of(align);
        let new_size = (ptr - block + size).next_multiple_of(self.gran);
        if new_size != bsize {
            let rest = block + new_size;
            let next = block + bsize;
            self.blocks.insert(rest, (bsize - new_size, false));
            self.prev.insert(rest, Some(block));
            self.prev.insert(next, Some(rest));
            self.link(rest, bsize - new_size);
        }
        self.blocks.insert(block, (new_size, true));
        self.block_of.insert(ptr, block);
        Some(ptr)
    }

    /// `Tlsf::deallocate`: merge with the next block if free, then the
    /// previous if free, and push the result onto its list's head.
    pub fn deallocate(&mut self, ptr: u64) {
        let mut block = self.block_of.remove(&ptr).expect("a live allocation");
        let (mut size, _) = self.blocks[&block];
        let next = block + size;
        if let Some(&(nsize, false)) = self.blocks.get(&next) {
            self.unlink(next, nsize);
            self.blocks.remove(&next);
            self.prev.remove(&next);
            size += nsize;
        }
        let after = block + size;
        if let Some(prev) = self.prev[&block]
            && let Some(&(psize, false)) = self.blocks.get(&prev)
        {
            self.unlink(prev, psize);
            self.blocks.remove(&block);
            self.prev.remove(&block);
            block = prev;
            size += psize;
        }
        self.blocks.insert(block, (size, false));
        self.link(block, size);
        self.prev.insert(after, Some(block));
    }

    /// `(holes, largest payload, free payload)` over the free blocks — what
    /// esp-alloc's `free()` walk (`max_payload_size`) sees.
    pub fn free_shape(&self) -> (u32, u64, u64) {
        let mut holes = 0;
        let mut largest = 0;
        let mut free = 0;
        for &(size, used) in self.blocks.values() {
            if used {
                continue;
            }
            holes += 1;
            largest = largest.max(size - self.hdr);
            free += size - self.hdr;
        }
        (holes, largest, free)
    }

    /// The largest `allocate(n, align ≤ header)` this pool would serve.
    pub fn largest_request(&self) -> u64 {
        if self.fl_bitmap == 0 {
            return 0;
        }
        let fl = 63 - self.fl_bitmap.leading_zeros() as usize;
        let sl = 63 - self.sl_bitmap[fl].leading_zeros() as usize;
        self.list_min(fl, sl) - self.hdr
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::alloc::Layout;
    use core::ptr::NonNull;

    #[test]
    fn the_model_places_every_block_where_real_rlsf_does_at_this_hosts_width() {
        // The real crate can only run at the host's width, so that is where
        // the model is checked: the same op sequence, every returned address
        // compared pool-relative, every refusal compared, and the free-block
        // walk compared at the end. The replay then runs the same logic at
        // the device's width.
        for seed in 1..=6u64 {
            let pool_size = 64 * 1024 + seed as usize * 1000;
            let (mut real, base, pool) = real_pool(pool_size);
            let mut model = TlsfModel::new(
                core::mem::size_of::<usize>(),
                usize::BITS as usize,
                0,
                pool_size as u64,
            );
            let mut rng = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15);
            let mut live: Vec<(NonNull<u8>, Layout, u64)> = Vec::new();
            for step in 0..6_000 {
                rng = rng
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                let roll = (rng >> 33) % 100;
                if roll < 55 || live.is_empty() {
                    let size = match (rng >> 20) % 10 {
                        0 => 1 + (rng >> 40) % 6000,
                        1..=3 => 1 + (rng >> 40) % 600,
                        _ => 1 + (rng >> 40) % 96,
                    } as usize;
                    let align = [1, 4, 8, 16, 32, 64, 256][((rng >> 12) % 7) as usize];
                    let layout = Layout::from_size_align(size, align).unwrap();
                    let got = real.allocate(layout);
                    let want = model.allocate(size as u64, align as u64);
                    match (got, want) {
                        (Some(p), Some(m)) => {
                            assert_eq!(
                                p.as_ptr() as u64 - base,
                                m,
                                "seed {seed} step {step}: {size} B align {align}"
                            );
                            live.push((p, layout, m));
                        }
                        (None, None) => {}
                        other => panic!("seed {seed} step {step}: {other:?}"),
                    }
                } else {
                    let i = (rng >> 8) as usize % live.len();
                    let (p, layout, m) = live.swap_remove(i);
                    unsafe { real.deallocate(p, layout.align()) };
                    model.deallocate(m);
                }
            }
            let mut real_free = Vec::new();
            for block in unsafe { real.iter_blocks(pool) } {
                if !block.is_occupied() {
                    real_free.push(block.max_payload_size() as u64);
                }
            }
            let (holes, largest, free) = model.free_shape();
            assert_eq!(holes as usize, real_free.len(), "seed {seed}");
            assert_eq!(largest, real_free.iter().copied().max().unwrap_or(0));
            assert_eq!(free, real_free.iter().sum::<u64>());
        }
    }

    #[test]
    fn device_geometry_is_a_16_byte_granule_and_an_8_byte_header() {
        // A 52 B request on a fresh 16-aligned pool: payload at +8, the block
        // is round16(8 + 52) = 64 B, so the next payload is at 64 + 8.
        let mut model = TlsfModel::new(DEVICE_WORD, ESP_ALLOC_FLLEN, 0, 4096);
        assert_eq!(model.allocate(52, 4), Some(8));
        assert_eq!(model.allocate(48, 4), Some(72));
        // One 4096 B pool: one free block of 4096 − 16 (the sentinel) − 128
        // used, whose payload is one header less.
        let (holes, largest, _) = model.free_shape();
        assert_eq!((holes, largest), (1, 4096 - 16 - 128 - 8));
    }

    #[test]
    fn the_largest_request_is_the_largest_blocks_list_floor() {
        // A 65,536 B region (the C6's dram2_seg): one free block of 65,520 B
        // on list (11, 31), whose floor is 64,512 B — so the largest request
        // the probe finds is 64,504 B, as the TLSF C6 image's heartbeat says.
        let model = TlsfModel::new(DEVICE_WORD, ESP_ALLOC_FLLEN, 0, 65_536);
        assert_eq!(model.free_shape().1, 65_512);
        assert_eq!(model.largest_request(), 64_504);
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
        let events = [
            TraceEventOwned::synthetic_alloc(1, 100_000, 4, Vec::new(), 10),
            TraceEventOwned::synthetic_marker("probe".to_string(), "I".to_string(), 20),
        ];
        let result = replay_tlsf(&events, &regions, &mut discounts);
        assert_eq!(result.would_oom, 1);
    }

    /// A real `rlsf` pool at this host's width over a leaked 4 KiB-aligned
    /// buffer: `(tlsf, base address, the pool slice iter_blocks wants)`.
    fn real_pool(
        size: usize,
    ) -> (
        rlsf::Tlsf<'static, usize, usize, 64, 32>,
        u64,
        NonNull<[u8]>,
    ) {
        let layout = Layout::from_size_align(size, 4096).unwrap();
        // SAFETY: non-zero size; leaked, so it outlives the `'static` pool.
        let raw = unsafe { ::alloc::alloc::alloc(layout) };
        let raw = NonNull::new(raw).expect("host allocation");
        let mut tlsf = rlsf::Tlsf::new();
        // SAFETY: a uniquely owned, leaked buffer.
        let len = unsafe { tlsf.insert_free_block_ptr(NonNull::slice_from_raw_parts(raw, size)) }
            .expect("a pool this size is accepted");
        (
            tlsf,
            raw.as_ptr() as u64,
            NonNull::slice_from_raw_parts(raw, len.get()),
        )
    }
}
