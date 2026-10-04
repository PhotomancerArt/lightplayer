//! One wasmtime [`Store`], one [`Memory`], and a first-fit heap over it for [`LpvmMemory`].
//!
//! **A heap over pre-grown memory.** The host runtime pre-grows the linear memory once in
//! [`WasmLpvmSharedRuntime::new`] to [`crate::options::WasmOptions::host_memory_pages`]
//! (default 64 MiB), and [`Memory::grow`] is never called again: cached host pointers in
//! [`lpvm::LpvmBuffer::native`] stay valid because the linear memory is never relocated.
//! Allocations that do not fit under that cap return [`AllocError::OutOfMemory`].
//!
//! `HostHeap` hands out blocks from a cursor that only rises into untouched memory, and
//! takes freed blocks back onto an address-ordered free list, coalescing neighbours; a
//! free block that reaches the cursor lowers it instead. `alloc` tries the free list
//! first-fit before it moves the cursor, so a workload that frees what it allocates
//! (every Studio render probe drops its render target) runs in bounded memory. Freed
//! bytes are zeroed, so memory a caller receives is zero, as it was when this was a bump
//! allocator over a fresh zero-filled memory.

use std::collections::{BTreeMap, HashMap};
use std::format;
use std::sync::{Arc, Mutex, MutexGuard};

use lpvm::{AllocError, LpvmBuffer, LpvmMemory};
use wasmtime::{Engine, Memory, MemoryType, Store};

use crate::error::WasmError;
use crate::module::EnvMemorySpec;

pub(crate) struct WasmLpvmSharedRuntimeInner {
    pub store: Store<()>,
    pub memory: Memory,
    heap: HostHeap,
}

pub(crate) struct WasmLpvmSharedRuntime {
    inner: Mutex<WasmLpvmSharedRuntimeInner>,
}

impl WasmLpvmSharedRuntime {
    pub(crate) fn new(engine: &Engine, host_memory_pages: u32) -> Result<Arc<Self>, WasmError> {
        let spec = EnvMemorySpec::engine_initial_for_host();
        let mem_ty = MemoryType::new(spec.initial_pages, spec.max_pages);
        let mut store = Store::new(engine, ());
        let memory = Memory::new(&mut store, mem_ty)
            .map_err(|e| WasmError::runtime(format!("Memory::new: {e}")))?;

        // Pre-grow once to the host budget so cached native pointers in
        // LpvmBuffer never observe a Memory::grow relocation. See module docs.
        let current_pages = memory.size(&store);
        let current_pages_u32 = u32::try_from(current_pages).map_err(|_| {
            WasmError::runtime(format!(
                "wasm linear memory size ({current_pages} pages) does not fit in u32"
            ))
        })?;
        if host_memory_pages > current_pages_u32 {
            let delta = u64::from(host_memory_pages - current_pages_u32);
            memory.grow(&mut store, delta).map_err(|e| {
                WasmError::runtime(format!("pre-grow to {host_memory_pages} pages failed: {e}"))
            })?;
        }

        let guest_reserve = usize::try_from(EnvMemorySpec::guest_reserve_bytes())
            .map_err(|_| WasmError::runtime("guest reserve size"))?;
        Ok(Arc::new(Self {
            inner: Mutex::new(WasmLpvmSharedRuntimeInner {
                store,
                memory,
                heap: HostHeap::new(guest_reserve),
            }),
        }))
    }

    pub(crate) fn lock(&self) -> MutexGuard<'_, WasmLpvmSharedRuntimeInner> {
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// [`LpvmMemory`] over shared wasmtime linear memory (a `HostHeap` over memory pre-grown at init).
///
/// See the module-level documentation.
pub(crate) struct WasmtimeLpvmMemory {
    runtime: Arc<WasmLpvmSharedRuntime>,
}

impl WasmtimeLpvmMemory {
    pub(crate) fn new(runtime: Arc<WasmLpvmSharedRuntime>) -> Self {
        Self { runtime }
    }
}

impl LpvmMemory for WasmtimeLpvmMemory {
    fn alloc(&self, size: usize, align: usize) -> Result<LpvmBuffer, AllocError> {
        if size == 0 {
            return Err(AllocError::InvalidSize);
        }
        if !align.is_power_of_two() {
            return Err(AllocError::InvalidSize);
        }
        let mut guard = self.runtime.lock();
        let mem = guard.memory;
        // Memory was pre-grown once at engine init; never grow again or cached
        // LpvmBuffer.native pointers go stale. Past the cap is OOM.
        let limit = mem.data_size(&guard.store);
        let aligned = guard.heap.alloc(size, align, limit)?;
        let native_base = mem.data_mut(&mut guard.store).as_mut_ptr();
        let native = unsafe { native_base.add(aligned) };
        Ok(LpvmBuffer::new(native, aligned as u64, size, align))
    }

    /// Return `buffer`'s block to the heap, zeroed.
    ///
    /// A buffer this heap does not hold live — freed twice, or never handed
    /// out by it — is ignored: nothing is zeroed and nothing returns to the
    /// free list, the rule the browser and native-JIT memories follow too.
    fn free(&self, buffer: LpvmBuffer) {
        let Ok(address) = usize::try_from(buffer.guest_base()) else {
            return;
        };
        let mut guard = self.runtime.lock();
        let Some((start, end)) = guard.heap.free(address) else {
            return;
        };
        let mem = guard.memory;
        mem.data_mut(&mut guard.store)[start..end].fill(0);
    }

    fn realloc(&self, _buffer: LpvmBuffer, _new_size: usize) -> Result<LpvmBuffer, AllocError> {
        // Not supported; use alloc + copy + free old.
        Err(AllocError::InvalidPointer)
    }
}

/// First-fit heap over `[base, limit)` of a linear memory that never moves.
///
/// Everything at or above `cursor` has never been handed out. Below it, every
/// byte is in exactly one live block or one free block. A block is the span
/// an allocation owns, from where its search started to the end of its bytes,
/// so the alignment padding in front of an allocation goes back with it.
struct HostHeap {
    cursor: usize,
    /// Free blocks below `cursor`, `start → end`: disjoint, never adjacent to
    /// each other (adjacent ones are merged), and none ends at `cursor` (that
    /// one lowers the cursor instead).
    free: BTreeMap<usize, usize>,
    /// Live allocations by the address handed out: `address → (start, end)`
    /// of the block it owns.
    live: HashMap<usize, (usize, usize)>,
}

impl HostHeap {
    fn new(base: usize) -> Self {
        Self {
            cursor: base,
            free: BTreeMap::new(),
            live: HashMap::new(),
        }
    }

    /// Place `size` bytes at a multiple of `align` below `limit`; the address.
    fn alloc(&mut self, size: usize, align: usize, limit: usize) -> Result<usize, AllocError> {
        let fit = self.free.iter().find_map(|(&start, &end)| {
            let aligned = round_up(start, align);
            let taken = aligned.checked_add(size)?;
            (taken <= end).then_some((start, end, aligned, taken))
        });
        if let Some((start, end, aligned, taken)) = fit {
            self.free.remove(&start);
            if taken < end {
                self.free.insert(taken, end);
            }
            self.live.insert(aligned, (start, taken));
            return Ok(aligned);
        }

        let start = self.cursor;
        let aligned = round_up(start, align);
        let taken = aligned.checked_add(size).ok_or(AllocError::InvalidSize)?;
        if taken > limit {
            return Err(AllocError::OutOfMemory);
        }
        self.cursor = taken;
        self.live.insert(aligned, (start, taken));
        Ok(aligned)
    }

    /// Release the allocation at `address`; the block it owned (to be zeroed
    /// by the caller), or `None` when nothing live is at `address`.
    fn free(&mut self, address: usize) -> Option<(usize, usize)> {
        let (start, end) = self.live.remove(&address)?;
        let mut merged_start = start;
        let mut merged_end = end;
        if let Some((&before_start, &before_end)) = self.free.range(..start).next_back()
            && before_end == start
        {
            self.free.remove(&before_start);
            merged_start = before_start;
        }
        if let Some(after_end) = self.free.remove(&end) {
            merged_end = after_end;
        }
        if merged_end == self.cursor {
            self.cursor = merged_start;
        } else {
            self.free.insert(merged_start, merged_end);
        }
        Some((start, end))
    }
}

fn round_up(value: usize, align: usize) -> usize {
    debug_assert!(align.is_power_of_two());
    let mask = align - 1;
    match value.checked_add(mask) {
        Some(v) => v & !mask,
        None => usize::MAX,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use lpvm::{LpvmBuffer, LpvmEngine, LpvmMemory};
    use wasmtime::Engine;

    use super::{WasmLpvmSharedRuntime, WasmtimeLpvmMemory};
    use crate::module::EnvMemorySpec;
    use crate::options::WasmOptions;
    use crate::rt_wasmtime::WasmLpvmEngine;

    /// (a) 10,000 × alloc+free of 8 KiB is 82 MB through a 64 MiB memory:
    /// only a memory that reuses what is freed gets to the end.
    #[test]
    fn alloc_free_churn_past_the_cap_reuses_memory() {
        let engine = WasmLpvmEngine::new(WasmOptions::default()).expect("engine");
        let memory = engine.memory();
        for i in 0..10_000 {
            let buffer = memory
                .alloc(8192, 4)
                .unwrap_or_else(|e| panic!("alloc #{i} of 8192 B: {e:?}"));
            memory.free(buffer);
        }
    }

    /// (b) Two adjacent freed blocks merge: their combined size fits where
    /// they were, and the cursor does not move.
    #[test]
    fn adjacent_frees_coalesce_into_one_block() {
        let (runtime, memory) = default_memory();
        let a = memory.alloc(256, 16).expect("a");
        let b = memory.alloc(256, 16).expect("b");
        let _fence = memory.alloc(16, 16).expect("fence above a and b");
        let cursor = cursor_of(&runtime);

        memory.free(a);
        memory.free(b);
        let ab = memory.alloc(512, 16).expect("a+b");

        assert_eq!(ab.guest_base(), a.guest_base(), "lands where a was");
        assert_eq!(cursor_of(&runtime), cursor, "cursor did not grow");
    }

    /// (c) A live allocation is never handed out twice: D takes the hole B
    /// left, and A and C read back exactly what was written.
    #[test]
    fn reuse_fills_the_hole_and_leaves_neighbours_alone() {
        let (_runtime, memory) = default_memory();
        let a = memory.alloc(1024, 4).expect("a");
        let b = memory.alloc(1024, 4).expect("b");
        let c = memory.alloc(1024, 4).expect("c");
        fill(a, 0xA1);
        fill(b, 0xB2);
        fill(c, 0xC3);

        memory.free(b);
        let d = memory.alloc(600, 4).expect("d");
        let d_start = d.guest_base();
        assert!(
            d_start >= b.guest_base() && d_start + 600 <= b.guest_base() + 1024,
            "d at {d_start} lies inside b's range"
        );
        assert!(bytes(d).iter().all(|&x| x == 0), "reused block is zeroed");
        fill(d, 0xD4);

        assert!(bytes(a).iter().all(|&x| x == 0xA1), "a untouched");
        assert!(bytes(c).iter().all(|&x| x == 0xC3), "c untouched");
    }

    /// (d) Freeing a buffer that is not live — the second free of the same
    /// buffer, or an address that was never handed out — is ignored: the
    /// block goes onto the free list once, so it is handed out once.
    #[test]
    fn double_free_and_unknown_buffers_are_ignored() {
        let (runtime, memory) = default_memory();
        let a = memory.alloc(64, 8).expect("a");
        let fence = memory.alloc(64, 8).expect("fence");
        fill(fence, 0xEE);

        memory.free(a);
        memory.free(a);
        // Inside the fence, but not an address the heap handed out.
        let stray = LpvmBuffer::new(
            unsafe { fence.native_ptr().add(8) },
            fence.guest_base() + 8,
            8,
            8,
        );
        memory.free(stray);
        assert!(bytes(fence).iter().all(|&x| x == 0xEE), "fence not zeroed");

        let cursor = cursor_of(&runtime);
        let x = memory.alloc(64, 8).expect("x");
        let y = memory.alloc(64, 8).expect("y");
        assert_eq!(x.guest_base(), a.guest_base(), "x reuses a");
        assert_ne!(y.guest_base(), x.guest_base(), "a was not freed twice");
        assert!(y.guest_base() >= cursor as u64, "y comes off the cursor");
        assert!(bytes(fence).iter().all(|&x| x == 0xEE), "fence still live");
    }

    /// The alignment `alloc` asks for holds on the free-list path too.
    #[test]
    fn reused_blocks_keep_the_requested_alignment() {
        let (_runtime, memory) = default_memory();
        let odd = memory.alloc(3, 1).expect("odd");
        let _fence = memory.alloc(1, 1).expect("fence");
        memory.free(odd);
        let _small = memory.alloc(1, 1).expect("small");
        let hole = memory.alloc(256, 1).expect("hole");
        let _fence2 = memory.alloc(1, 1).expect("fence2");
        memory.free(hole);
        let aligned = memory.alloc(64, 64).expect("aligned");
        assert_eq!(aligned.guest_base() % 64, 0);
    }

    /// Freeing everything gives the whole region back: the cursor returns to
    /// the guest reserve, whatever order the frees came in.
    #[test]
    fn freeing_everything_returns_the_cursor_to_the_base() {
        let (runtime, memory) = default_memory();
        let base = cursor_of(&runtime);
        let buffers: Vec<_> = (1..=12)
            .map(|i| memory.alloc(i * 40, 1 << (i % 5)).expect("alloc"))
            .collect();
        for i in [3, 0, 7, 11, 1, 5, 9, 2, 10, 4, 8, 6] {
            memory.free(buffers[i]);
        }
        let runtime = runtime.lock();
        assert_eq!(runtime.heap.cursor, base);
        assert!(runtime.heap.free.is_empty());
        assert!(runtime.heap.live.is_empty());
    }

    fn default_memory() -> (Arc<WasmLpvmSharedRuntime>, WasmtimeLpvmMemory) {
        let runtime = WasmLpvmSharedRuntime::new(
            &Engine::default(),
            WasmOptions::default().host_memory_pages,
        )
        .expect("runtime");
        assert_eq!(
            cursor_of(&runtime),
            EnvMemorySpec::guest_reserve_bytes() as usize
        );
        let memory = WasmtimeLpvmMemory::new(Arc::clone(&runtime));
        (runtime, memory)
    }

    fn cursor_of(runtime: &WasmLpvmSharedRuntime) -> usize {
        runtime.lock().heap.cursor
    }

    fn fill(buffer: LpvmBuffer, byte: u8) {
        unsafe { core::ptr::write_bytes(buffer.native_ptr(), byte, buffer.size()) };
    }

    fn bytes(buffer: LpvmBuffer) -> Vec<u8> {
        unsafe { core::slice::from_raw_parts(buffer.native_ptr(), buffer.size()) }.to_vec()
    }
}
