//! E10 spike (research only, never to `main`): a per-project arena for the
//! resolver's own tables, through `allocator_api`.
//!
//! [`ProjectAlloc`] is the allocator parameter the resolver's collections
//! carry. With the `project-arena` feature off it is [`Global`] and nothing
//! changes. With it on it is a [`ProjectArena`] handle: chunks of
//! [`CHUNK_BYTES`] taken from the global heap, bump allocation inside them, a
//! free of the most recent allocation rolls the bump back, any other free is
//! counted as dead bytes and reclaimed only when the arena drops — which is
//! when the last handle (every collection that allocated in it) drops.
//!
//! The counters are the "tracking falls out of arenas" claim: one arena per
//! [`crate::Engine`], so its bytes are the project's resolver bytes by
//! construction, with no call-site attribution.
//!
//! The default handle ([`ProjectArena::default`]) is *detached*: it passes
//! straight through to [`Global`]. That is what `Resolver::new()` uses, so
//! the per-frame `mem::replace(&mut self.resolver, Resolver::new())`
//! placeholders stay allocation-free.

#[cfg(not(feature = "project-arena"))]
pub use alloc::alloc::Global as ProjectAlloc;

#[cfg(feature = "project-arena")]
pub use arena::{ArenaStats, ProjectArena, ProjectArena as ProjectAlloc, arenas_alive};

/// A fresh allocator for one engine (one loaded project).
#[cfg(feature = "project-arena")]
pub fn project_alloc_for_engine() -> ProjectAlloc {
    ProjectArena::new()
}

/// A fresh allocator for one engine (one loaded project).
#[cfg(not(feature = "project-arena"))]
pub fn project_alloc_for_engine() -> ProjectAlloc {
    alloc::alloc::Global
}

/// Log the arena's counters with a reason (no-op without the feature).
#[cfg(feature = "project-arena")]
pub fn log_arena(alloc: &ProjectAlloc, why: &str, epoch: u64) {
    if let Some(stats) = alloc.stats() {
        log::info!(
            "[e10] arena #{} {} epoch={} chunks={} held={} used={} live={} dead={} peak_live={} allocs={} frees={} rollbacks={} grows_in_place={} alive={}",
            stats.id,
            why,
            epoch,
            stats.chunks,
            stats.held_bytes,
            stats.used_bytes,
            stats.live_bytes,
            stats.dead_bytes,
            stats.peak_live_bytes,
            stats.allocs,
            stats.frees,
            stats.rollbacks,
            stats.grows_in_place,
            arenas_alive(),
        );
    }
}

/// Log the arena's counters with a reason (no-op without the feature).
#[cfg(not(feature = "project-arena"))]
pub fn log_arena(_alloc: &ProjectAlloc, _why: &str, _epoch: u64) {}

#[cfg(feature = "project-arena")]
mod arena {
    use alloc::alloc::Global;
    use alloc::rc::Rc;
    use alloc::vec::Vec;
    use core::alloc::{AllocError, Allocator, Layout};
    use core::cell::{Cell, RefCell};
    use core::ptr::NonNull;
    use core::sync::atomic::{AtomicU32, Ordering};

    /// One page of the arena. A request over half a page gets a chunk of
    /// its own (rounded up to 8 B) and leaves the current page current.
    pub const CHUNK_BYTES: usize = 4096;
    const CHUNK_ALIGN: usize = 8;

    static ARENAS_ALIVE: AtomicU32 = AtomicU32::new(0);
    static NEXT_ID: AtomicU32 = AtomicU32::new(1);

    /// How many arenas exist right now. After a project switch this should
    /// be one (the new project's); more means a handle outlived its project.
    pub fn arenas_alive() -> u32 {
        ARENAS_ALIVE.load(Ordering::Relaxed)
    }

    #[derive(Copy, Clone, Debug, Default)]
    pub struct ArenaStats {
        pub id: u32,
        pub chunks: u32,
        /// Bytes taken from the global heap (all chunks).
        pub held_bytes: u32,
        /// Bytes bumped past (live + dead + alignment padding).
        pub used_bytes: u32,
        pub live_bytes: u32,
        /// Freed inside the arena but not reclaimable until it drops.
        pub dead_bytes: u32,
        pub peak_live_bytes: u32,
        pub allocs: u32,
        pub frees: u32,
        pub rollbacks: u32,
        pub grows_in_place: u32,
    }

    struct ArenaInner {
        /// Every chunk, for the drop. Lives in the global heap.
        chunks: RefCell<Vec<(NonNull<u8>, Layout)>>,
        cur_end: Cell<usize>,
        bump: Cell<usize>,
        /// Start and size of the most recent allocation in the current
        /// chunk, for rollback and in-place growth (0 = none).
        last: Cell<usize>,
        stats: Cell<ArenaStats>,
    }

    impl ArenaInner {
        fn update(&self, f: impl FnOnce(&mut ArenaStats)) {
            let mut stats = self.stats.get();
            f(&mut stats);
            self.stats.set(stats);
        }

        fn new_chunk(&self, size: usize) -> Result<NonNull<u8>, AllocError> {
            let layout = Layout::from_size_align(size, CHUNK_ALIGN).map_err(|_| AllocError)?;
            let ptr = Global.allocate(layout)?.cast::<u8>();
            self.chunks.borrow_mut().push((ptr, layout));
            self.update(|s| {
                s.chunks += 1;
                s.held_bytes += size as u32;
            });
            Ok(ptr)
        }

        fn alloc(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
            let size = layout.size();
            let align = layout.align();
            if size == 0 {
                let dangling = NonNull::new(align as *mut u8).ok_or(AllocError)?;
                return Ok(NonNull::slice_from_raw_parts(dangling, 0));
            }
            if align > CHUNK_ALIGN || size > CHUNK_BYTES / 2 {
                // A chunk of its own: never the bump chunk, never rolled back.
                let rounded = (size + CHUNK_ALIGN - 1) & !(CHUNK_ALIGN - 1);
                let ptr = if align > CHUNK_ALIGN {
                    let l = Layout::from_size_align(rounded, align).map_err(|_| AllocError)?;
                    let p = Global.allocate(l)?.cast::<u8>();
                    self.chunks.borrow_mut().push((p, l));
                    self.update(|s| {
                        s.chunks += 1;
                        s.held_bytes += rounded as u32;
                    });
                    p
                } else {
                    self.new_chunk(rounded)?
                };
                self.update(|s| {
                    s.used_bytes += rounded as u32;
                    s.allocs += 1;
                    s.live_bytes += size as u32;
                    s.peak_live_bytes = s.peak_live_bytes.max(s.live_bytes);
                });
                return Ok(NonNull::slice_from_raw_parts(ptr, size));
            }
            let mut start = (self.bump.get() + align - 1) & !(align - 1);
            if self.bump.get() == 0 || start + size > self.cur_end.get() {
                let chunk = self.new_chunk(CHUNK_BYTES)?.as_ptr() as usize;
                // The tail of the old page is left as padding (counted in
                // `held - used`).
                self.cur_end.set(chunk + CHUNK_BYTES);
                self.bump.set(chunk);
                start = chunk;
            }
            let padding = start - self.bump.get();
            self.bump.set(start + size);
            self.last.set(start);
            self.update(|s| {
                s.used_bytes += (padding + size) as u32;
                s.allocs += 1;
                s.live_bytes += size as u32;
                s.peak_live_bytes = s.peak_live_bytes.max(s.live_bytes);
            });
            let ptr = NonNull::new(start as *mut u8).ok_or(AllocError)?;
            Ok(NonNull::slice_from_raw_parts(ptr, size))
        }

        fn free(&self, ptr: NonNull<u8>, layout: Layout) {
            let size = layout.size();
            if size == 0 {
                return;
            }
            let addr = ptr.as_ptr() as usize;
            if addr == self.last.get() && addr + size == self.bump.get() {
                self.bump.set(addr);
                self.last.set(0);
                self.update(|s| {
                    s.frees += 1;
                    s.rollbacks += 1;
                    s.live_bytes -= size as u32;
                    s.used_bytes -= size as u32;
                });
                return;
            }
            self.update(|s| {
                s.frees += 1;
                s.live_bytes -= size as u32;
                s.dead_bytes += size as u32;
            });
        }

        /// Grow the most recent allocation where it stands, if the page has
        /// room. `None` = not possible, take the copy path.
        fn grow_in_place(
            &self,
            ptr: NonNull<u8>,
            old: Layout,
            new: Layout,
        ) -> Option<NonNull<[u8]>> {
            let addr = ptr.as_ptr() as usize;
            if old.size() == 0
                || addr != self.last.get()
                || addr + old.size() != self.bump.get()
                || new.align() > old.align()
                || addr + new.size() > self.cur_end.get()
            {
                return None;
            }
            let extra = new.size() - old.size();
            self.bump.set(addr + new.size());
            self.update(|s| {
                s.used_bytes += extra as u32;
                s.live_bytes += extra as u32;
                s.peak_live_bytes = s.peak_live_bytes.max(s.live_bytes);
                s.grows_in_place += 1;
            });
            Some(NonNull::slice_from_raw_parts(ptr, new.size()))
        }
    }

    impl Drop for ArenaInner {
        fn drop(&mut self) {
            let stats = self.stats.get();
            ARENAS_ALIVE.fetch_sub(1, Ordering::Relaxed);
            log::info!(
                "[e10] arena #{} dropped: chunks={} held={} used={} live_at_drop={} dead={} peak_live={} allocs={} frees={} alive={}",
                stats.id,
                stats.chunks,
                stats.held_bytes,
                stats.used_bytes,
                stats.live_bytes,
                stats.dead_bytes,
                stats.peak_live_bytes,
                stats.allocs,
                stats.frees,
                arenas_alive(),
            );
            for (ptr, layout) in self.chunks.get_mut().drain(..) {
                // SAFETY: each chunk was allocated by `Global` with exactly
                // this layout and is freed once, here.
                unsafe { Global.deallocate(ptr, layout) };
            }
        }
    }

    /// A handle to one project's arena; `Default` is detached (passes
    /// through to [`Global`]). Cloning shares the arena.
    #[derive(Clone, Default)]
    pub struct ProjectArena(Option<Rc<ArenaInner>>);

    impl core::fmt::Debug for ProjectArena {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            match self.stats() {
                Some(s) => write!(f, "ProjectArena(#{}, live {})", s.id, s.live_bytes),
                None => f.write_str("ProjectArena(detached)"),
            }
        }
    }

    impl ProjectArena {
        pub fn new() -> Self {
            ARENAS_ALIVE.fetch_add(1, Ordering::Relaxed);
            let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
            Self(Some(Rc::new(ArenaInner {
                chunks: RefCell::new(Vec::new()),
                cur_end: Cell::new(0),
                bump: Cell::new(0),
                last: Cell::new(0),
                stats: Cell::new(ArenaStats {
                    id,
                    ..ArenaStats::default()
                }),
            })))
        }

        pub fn stats(&self) -> Option<ArenaStats> {
            self.0.as_ref().map(|inner| inner.stats.get())
        }
    }

    // SAFETY: memory handed out stays valid until freed or until the arena
    // drops; the arena drops only when the last handle does, and every
    // collection that allocated holds a handle. A clone shares the arena, so
    // memory from one clone may be freed through another. The detached
    // handle is `Global`.
    unsafe impl Allocator for ProjectArena {
        fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
            match &self.0 {
                Some(inner) => inner.alloc(layout),
                None => Global.allocate(layout),
            }
        }

        unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
            match &self.0 {
                Some(inner) => inner.free(ptr, layout),
                // SAFETY: forwarded from the caller's contract.
                None => unsafe { Global.deallocate(ptr, layout) },
            }
        }

        unsafe fn grow(
            &self,
            ptr: NonNull<u8>,
            old_layout: Layout,
            new_layout: Layout,
        ) -> Result<NonNull<[u8]>, AllocError> {
            let Some(inner) = &self.0 else {
                // SAFETY: forwarded from the caller's contract.
                return unsafe { Global.grow(ptr, old_layout, new_layout) };
            };
            if let Some(grown) = inner.grow_in_place(ptr, old_layout, new_layout) {
                return Ok(grown);
            }
            let new = inner.alloc(new_layout)?;
            // SAFETY: both regions are valid for `old_layout.size()` bytes
            // and distinct (the new one was just bumped past the old).
            unsafe {
                core::ptr::copy_nonoverlapping(
                    ptr.as_ptr(),
                    new.cast::<u8>().as_ptr(),
                    old_layout.size(),
                );
            }
            inner.free(ptr, old_layout);
            Ok(new)
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use alloc::vec::Vec;

        #[test]
        fn vec_growth_stays_in_place_and_drop_reclaims_everything() {
            let arena = ProjectArena::new();
            {
                let mut v: Vec<u32, ProjectArena> = Vec::new_in(arena.clone());
                for i in 0..200 {
                    v.push(i);
                }
                assert_eq!(v.iter().sum::<u32>(), 199 * 200 / 2);
                let s = arena.stats().unwrap();
                assert!(s.grows_in_place > 0);
                assert_eq!(s.dead_bytes, 0);
            }
            let s = arena.stats().unwrap();
            assert_eq!(s.live_bytes, 0);
        }

        #[test]
        fn non_top_free_is_dead_until_drop() {
            let arena = ProjectArena::new();
            let a = Rc::new_in(1u64, arena.clone());
            let b = Rc::new_in(2u64, arena.clone());
            drop(a);
            let s = arena.stats().unwrap();
            assert!(s.dead_bytes > 0);
            drop(b);
            assert_eq!(arena.stats().unwrap().live_bytes, 0);
        }

        #[test]
        fn detached_handle_is_global() {
            let v: Vec<u8, ProjectArena> = Vec::with_capacity_in(16, ProjectArena::default());
            assert!(v.capacity() >= 16);
            assert!(ProjectArena::default().stats().is_none());
        }
    }
}
