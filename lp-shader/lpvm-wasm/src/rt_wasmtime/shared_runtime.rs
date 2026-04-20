//! One wasmtime [`Store`], one [`Memory`], bump sub-region for [`LpvmMemory`].
//!
//! **Bump over pre-grown memory.** [`WasmtimeLpvmMemory`] only advances a cursor;
//! [`Memory::grow`] is never called after the engine is constructed. The host runtime
//! pre-grows the linear memory once in [`WasmLpvmSharedRuntime::new`] to
//! [`crate::options::WasmOptions::host_memory_pages`] (default 64 MiB). Allocations beyond that cap
//! return [`AllocError::OutOfMemory`].
//!
//! This is the safe path for production hosts: cached host pointers in
//! [`lpvm::LpvmBuffer::native`] stay valid because the underlying linear memory is
//! never relocated. The bump-only allocator does not reuse memory; if a real workload
//! exhausts the cap, raise [`crate::options::WasmOptions::host_memory_pages`] or revisit the allocator.

use std::format;
use std::sync::{Arc, Mutex, MutexGuard};

use lpvm::{AllocError, LpvmBuffer, LpvmMemory};
use wasmtime::{Engine, Memory, MemoryType, Store};

use crate::error::WasmError;
use crate::module::EnvMemorySpec;

pub(crate) struct WasmLpvmSharedRuntimeInner {
    pub store: Store<()>,
    pub memory: Memory,
    bump_cursor: usize,
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
                bump_cursor: guest_reserve,
            }),
        }))
    }

    pub(crate) fn lock(&self) -> MutexGuard<'_, WasmLpvmSharedRuntimeInner> {
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// [`LpvmMemory`] over shared wasmtime linear memory (bump-only; memory is pre-grown at init).
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
        let aligned = round_up(guard.bump_cursor, align);
        let end = aligned.checked_add(size).ok_or(AllocError::InvalidSize)?;

        // Memory was pre-grown once at engine init; never grow again or cached
        // LpvmBuffer.native pointers go stale. Past the cap is OOM.
        if end > mem.data_size(&guard.store) {
            return Err(AllocError::OutOfMemory);
        }

        guard.bump_cursor = end;
        let native_base = mem.data_mut(&mut guard.store).as_mut_ptr();
        let native = unsafe { native_base.add(aligned) };
        Ok(LpvmBuffer::new(native, aligned as u64, size, align))
    }

    fn free(&self, _buffer: LpvmBuffer) {
        // Bump semantics: memory is not reused.
    }

    fn realloc(&self, _buffer: LpvmBuffer, _new_size: usize) -> Result<LpvmBuffer, AllocError> {
        // Not supported for bump allocator; use alloc + copy + free old.
        Err(AllocError::InvalidPointer)
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
