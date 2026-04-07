//! One wasmtime [`Store`], one [`Memory`], bump sub-region for [`LpvmMemory`].
//!
//! **Bump + grow:** [`WasmtimeLpvmMemory`] only advances a cursor and calls [`Memory::grow`] when
//! the bump runs past the current size. That is acceptable for now because the wasmtime engine
//! path is used from **short-lived host tests**, not long-running production services. In a
//! long-lived process, a monotonic bump with unbounded growth would be a poor default (no reuse,
//! memory only ever expands); a future design would want reuse, caps, or a different strategy.

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
    pub(crate) fn new(engine: &Engine) -> Result<Arc<Self>, WasmError> {
        let spec = EnvMemorySpec::engine_initial_for_host();
        let mem_ty = MemoryType::new(spec.initial_pages, spec.max_pages);
        let mut store = Store::new(engine, ());
        let memory = Memory::new(&mut store, mem_ty)
            .map_err(|e| WasmError::runtime(format!("Memory::new: {e}")))?;
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

/// [`LpvmMemory`] over shared wasmtime linear memory (bump + [`Memory::grow`]).
///
/// See the module-level documentation for why bump-only allocation is acceptable on this path today.
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

        let page =
            usize::try_from(EnvMemorySpec::WASM_PAGE_SIZE).map_err(|_| AllocError::InvalidSize)?;
        let mut cur_len = mem.data_size(&guard.store);
        while end > cur_len {
            let need = end - cur_len;
            let delta_pages_u64 =
                u64::try_from((need + page - 1) / page).map_err(|_| AllocError::InvalidSize)?;
            if mem.grow(&mut guard.store, delta_pages_u64).is_err() {
                return Err(AllocError::OutOfMemory);
            }
            cur_len = mem.data_size(&guard.store);
            if end > cur_len {
                return Err(AllocError::OutOfMemory);
            }
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
