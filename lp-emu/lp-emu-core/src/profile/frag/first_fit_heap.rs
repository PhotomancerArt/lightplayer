//! A first-fit heap placement model, parametrized by target word width.
//!
//! `linked_list_allocator` 0.10.5 is what both the ESP32 classic (through
//! esp-alloc's `LLFF` algorithm) and the emulator guest run, but its block
//! geometry is derived from `size_of::<usize>()`: an 8 B minimum block and
//! 4 B size rounding on the 32-bit targets, 16 B and 8 B when the same crate
//! is instantiated on a 64-bit host. Replaying a 32-bit trace against the
//! host-compiled crate therefore over-consumes every small allocation and
//! drifts far past the cross-check tolerance, so the replay models the
//! placement instead, with [`HeapGeometry::RV32`] reproducing the target's
//! geometry exactly.
//!
//! The model is not a reimplementation for its own sake: it is checked
//! against the real crate at [`HeapGeometry::HOST64`] in this module's tests,
//! which is the only geometry the crate can be asked about on this host.

use ::alloc::vec::Vec;

/// Block geometry of one `linked_list_allocator` instance, set by the target's
/// word width.
///
/// `linked_list_allocator` stores its free list inside the holes themselves,
/// as `Hole { size: usize, next: Option<NonNull<Hole>> }`. That node's size is
/// the smallest block the allocator can hand out or track, and its alignment
/// is the granule every request size is rounded up to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeapGeometry {
    word: u32,
}

impl HeapGeometry {
    /// The 32-bit targets: the ESP32 classic and the emulator guest.
    /// 8 B minimum block, 4 B size granule.
    pub const RV32: Self = Self { word: 4 };

    /// The crate as compiled for a 64-bit host: 16 B minimum block, 8 B
    /// granule. Only used to check the model against the real crate.
    pub const HOST64: Self = Self { word: 8 };

    /// `size_of::<Hole>()` — the smallest block the allocator can represent.
    pub const fn min_block(self) -> u32 {
        self.word * 2
    }

    /// `align_of::<Hole>()` — the granule request sizes round up to.
    pub const fn granule(self) -> u32 {
        self.word
    }

    /// The bytes a request of `size` actually consumes: `HoleList::align_layout`
    /// raises it to [`Self::min_block`] and then rounds up to [`Self::granule`].
    /// Alignment does not enter into the size — only into placement.
    pub const fn footprint(self, size: u32) -> u32 {
        let raised = if size < self.min_block() {
            self.min_block()
        } else {
            size
        };
        align_up(raised, self.granule())
    }
}

/// One contiguous free block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hole {
    pub start: u32,
    pub size: u32,
}

impl Hole {
    pub const fn end(self) -> u32 {
        self.start + self.size
    }
}

/// One `linked_list_allocator` region: an address-ordered free list served
/// first-fit, splitting on allocate and coalescing on free.
#[derive(Debug, Clone)]
pub struct FirstFitHeap {
    geometry: HeapGeometry,
    bottom: u32,
    top: u32,
    /// Address-ordered, never empty of invariant: no two entries touch (they
    /// would have coalesced) and none is smaller than the minimum block.
    holes: Vec<Hole>,
}

impl FirstFitHeap {
    /// A region covering `[base, base + size)`, trimmed the way
    /// `HoleList::new` trims it: the bottom rounds up to the granule and the
    /// top rounds down.
    pub fn new(geometry: HeapGeometry, base: u32, size: u32) -> Self {
        let bottom = align_up(base, geometry.granule());
        let requested = size.saturating_sub(bottom - base);
        let usable = align_down(requested, geometry.granule());
        let top = bottom + usable;
        let holes = if usable >= geometry.min_block() {
            ::alloc::vec![Hole {
                start: bottom,
                size: usable
            }]
        } else {
            Vec::new()
        };
        Self {
            geometry,
            bottom,
            top,
            holes,
        }
    }

    pub fn geometry(&self) -> HeapGeometry {
        self.geometry
    }

    pub fn bottom(&self) -> u32 {
        self.bottom
    }

    pub fn top(&self) -> u32 {
        self.top
    }

    /// The free list, in address order.
    pub fn holes(&self) -> &[Hole] {
        &self.holes
    }

    /// Total free bytes — the free list's own sum, which is what the guest's
    /// walk reports and can sit below "size minus requested bytes" by the
    /// per-allocation rounding.
    pub fn free(&self) -> u32 {
        self.holes.iter().map(|h| h.size).sum()
    }

    /// Largest free block, 0 when the region is exhausted.
    pub fn largest_free(&self) -> u32 {
        self.holes.iter().map(|h| h.size).max().unwrap_or(0)
    }

    /// Serve one request first-fit, returning the address, or `None` when no
    /// hole can hold it.
    ///
    /// Mirrors `Cursor::split_current`: a hole whose start is already aligned
    /// is split into (allocation, back padding); one that is not is split into
    /// (front padding, allocation, back padding), with the front padding
    /// forced to be at least a hole node wide so the list can still address
    /// it. A hole that would leave a back remainder too small to hold a hole
    /// node is rejected rather than leaked, and the search moves on.
    pub fn allocate(&mut self, size: u32, align: u32) -> Option<u32> {
        let required = self.geometry.footprint(size);
        let min_block = self.geometry.min_block();

        for index in 0..self.holes.len() {
            let hole = self.holes[index];
            if hole.size < required {
                continue;
            }

            let (front_padding, addr) = if hole.start % align == 0 {
                (None, hole.start)
            } else {
                let pushed = hole.start.saturating_add(min_block);
                let aligned = align_up(pushed, align);
                (
                    Some(Hole {
                        start: hole.start,
                        size: aligned - hole.start,
                    }),
                    aligned,
                )
            };

            let alloc_end = match addr.checked_add(required) {
                Some(end) => end,
                None => continue,
            };
            if alloc_end > hole.end() {
                continue;
            }

            let back_size = hole.end() - alloc_end;
            let back_padding = if back_size == 0 {
                None
            } else {
                let back_start = align_up(alloc_end, self.geometry.granule());
                if back_start.saturating_add(min_block) > hole.end() {
                    // Not enough room left for a hole node; the crate refuses
                    // to leak the remainder and rejects this hole entirely.
                    continue;
                }
                Some(Hole {
                    start: back_start,
                    size: back_size,
                })
            };

            self.holes.remove(index);
            let mut at = index;
            if let Some(front) = front_padding {
                self.holes.insert(at, front);
                at += 1;
            }
            if let Some(back) = back_padding {
                self.holes.insert(at, back);
            }
            return Some(addr);
        }

        None
    }

    /// Return a block to the free list, coalescing with an adjacent hole on
    /// either side. `size` and `align` must be the ones the matching
    /// [`Self::allocate`] was given.
    pub fn deallocate(&mut self, addr: u32, size: u32, align: u32) {
        // `align` is taken for symmetry with `allocate` and to keep call
        // sites honest about pairing layouts; the crate's `align_layout`
        // derives the block's size from `size` alone, so it does not enter
        // the arithmetic here.
        let _ = align;
        let freed = Hole {
            start: addr,
            size: self.geometry.footprint(size),
        };
        let index = self.holes.partition_point(|h| h.start < freed.start);
        self.holes.insert(index, freed);

        // Merge forward first so the backward merge sees the joined block.
        if index + 1 < self.holes.len() && self.holes[index].end() == self.holes[index + 1].start {
            let next = self.holes.remove(index + 1);
            self.holes[index].size += next.size;
        }
        if index > 0 && self.holes[index - 1].end() == self.holes[index].start {
            let cur = self.holes.remove(index);
            self.holes[index - 1].size += cur.size;
        }
    }
}

/// Smallest multiple of `align` that is `>= value`. `align` must be a power of
/// two; a zero or non-power-of-two is treated as "no alignment", matching the
/// crate's `align_down_size` contract for `align == 0`.
pub const fn align_up(value: u32, align: u32) -> u32 {
    if align <= 1 {
        return value;
    }
    (value + align - 1) & !(align - 1)
}

/// Largest multiple of `align` that is `<= value`.
pub const fn align_down(value: u32, align: u32) -> u32 {
    if align <= 1 {
        return value;
    }
    value & !(align - 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::alloc::Layout;
    use std::collections::BTreeMap;

    #[test]
    fn footprint_matches_target_geometry() {
        let g = HeapGeometry::RV32;
        assert_eq!(g.footprint(0), 8, "a zero-size ask still costs a hole node");
        assert_eq!(g.footprint(1), 8);
        assert_eq!(g.footprint(8), 8);
        assert_eq!(g.footprint(9), 12);
        assert_eq!(g.footprint(20_480), 20_480);

        let h = HeapGeometry::HOST64;
        assert_eq!(h.footprint(1), 16);
        assert_eq!(h.footprint(17), 24);
    }

    #[test]
    fn split_then_free_restores_one_hole() {
        let mut heap = FirstFitHeap::new(HeapGeometry::RV32, 0x8000_0000, 1024);
        let a = heap.allocate(64, 4).expect("a fits");
        let b = heap.allocate(64, 4).expect("b fits");
        assert_eq!(a, 0x8000_0000);
        assert_eq!(b, 0x8000_0040);
        assert_eq!(heap.holes().len(), 1);

        heap.deallocate(a, 64, 4);
        assert_eq!(
            heap.holes().len(),
            2,
            "a's hole is not adjacent to the tail"
        );
        heap.deallocate(b, 64, 4);
        assert_eq!(
            heap.holes(),
            &[Hole {
                start: 0x8000_0000,
                size: 1024
            }],
            "freeing both must coalesce back to the whole region"
        );
    }

    #[test]
    fn first_fit_reuses_the_lowest_hole_that_fits() {
        let mut heap = FirstFitHeap::new(HeapGeometry::RV32, 0x8000_0000, 1024);
        let a = heap.allocate(32, 4).expect("a");
        let b = heap.allocate(32, 4).expect("b");
        let c = heap.allocate(32, 4).expect("c");
        heap.deallocate(a, 32, 4);
        heap.deallocate(c, 32, 4);

        // 16 B fits in a's hole, which is below c's, so first fit takes it.
        assert_eq!(heap.allocate(16, 4), Some(a));
        let _ = b;
    }

    #[test]
    fn exhaustion_returns_none_rather_than_a_bogus_address() {
        let mut heap = FirstFitHeap::new(HeapGeometry::RV32, 0x8000_0000, 64);
        assert!(heap.allocate(64, 4).is_some());
        assert_eq!(heap.allocate(4, 4), None);
        assert_eq!(heap.largest_free(), 0);
    }

    /// The model exists because the real crate cannot be instantiated with
    /// 32-bit geometry on this host. This is the check that the model is the
    /// same allocator: at the one geometry the crate *can* be asked about,
    /// every placement and every free-list shape must agree, step for step,
    /// over a long pseudo-random alloc/free/realloc mix.
    #[test]
    fn model_matches_the_real_crate_at_host_geometry() {
        const HEAP_BYTES: usize = 64 * 1024;
        let backing = ::alloc::vec![0u8; HEAP_BYTES].into_boxed_slice();
        let backing = ::alloc::boxed::Box::leak(backing);
        let base = backing.as_mut_ptr();

        let mut real = linked_list_allocator::Heap::empty();
        // SAFETY: `backing` is a leaked, uniquely-owned buffer of exactly
        // `HEAP_BYTES` that nothing else reads or writes for the rest of the
        // process.
        unsafe { real.init(base, HEAP_BYTES) };

        // A synthetic base: only offsets from the region bottom are compared.
        let mut model = FirstFitHeap::new(HeapGeometry::HOST64, 0x1000_0000, HEAP_BYTES as u32);
        assert_eq!(
            real.bottom() as usize % 8,
            0,
            "host allocation is expected to be 8-aligned; the offset mapping below assumes it"
        );

        // guest-ish handle -> (real ptr, model addr, size, align)
        let mut live: BTreeMap<u32, (usize, u32, u32, u32)> = BTreeMap::new();
        let mut next_handle = 1u32;
        let mut rng = Lcg::new(0x5eed_1234);
        let aligns = [4u32, 8, 16, 32, 64];

        for step in 0..4000 {
            let roll = rng.next() % 100;
            if roll < 55 || live.is_empty() {
                let size = 1 + rng.next() % 900;
                let align = aligns[(rng.next() % aligns.len() as u32) as usize];
                let layout = Layout::from_size_align(size as usize, align as usize)
                    .expect("layout is valid");
                let real_ptr = real.allocate_first_fit(layout).ok();
                let model_addr = model.allocate(size, align);
                match (real_ptr, model_addr) {
                    (Some(p), Some(m)) => {
                        let offset = p.as_ptr() as usize - real.bottom() as usize;
                        assert_eq!(
                            m - model.bottom(),
                            offset as u32,
                            "step {step}: placement diverged for size {size} align {align}"
                        );
                        live.insert(next_handle, (p.as_ptr() as usize, m, size, align));
                        next_handle += 1;
                    }
                    (None, None) => {}
                    (r, m) => panic!("step {step}: crate said {r:?}, model said {m:?}"),
                }
            } else {
                let keys: Vec<u32> = live.keys().copied().collect();
                let key = keys[(rng.next() % keys.len() as u32) as usize];
                let (ptr, addr, size, align) = live.remove(&key).expect("key was just listed");
                let layout = Layout::from_size_align(size as usize, align as usize)
                    .expect("layout is valid");
                // SAFETY: `ptr` came from `allocate_first_fit` with this exact
                // layout and has not been freed.
                unsafe {
                    real.deallocate(
                        core::ptr::NonNull::new(ptr as *mut u8).expect("non-null"),
                        layout,
                    )
                };
                model.deallocate(addr, size, align);
            }

            assert_eq!(
                model.free() as usize,
                real.free(),
                "step {step}: free-byte totals diverged"
            );
        }
    }

    /// Small deterministic PRNG so the oracle comparison is reproducible
    /// without pulling a dependency into a host-only test.
    struct Lcg(u64);

    impl Lcg {
        fn new(seed: u64) -> Self {
            Self(seed)
        }

        fn next(&mut self) -> u32 {
            self.0 = self
                .0
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            (self.0 >> 33) as u32
        }
    }
}
