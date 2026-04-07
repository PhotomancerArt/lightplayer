//! Host wasm module linear memory + real [`LpvmMemory`] for the browser target.
//!
//! Shaders import the **same** [`WebAssembly::Memory`] as this wasm instance (`wasm_bindgen::memory()`),
//! so host Rust and guest shaders share one linear address space. [`LpvmBuffer::guest_base`] is the
//! byte offset; [`LpvmBuffer::native_ptr`] is that offset as a wasm32 linear-memory pointer (direct
//! access from the host module).
//!
//! This implementation uses the **Rust global allocator** for real `alloc`/`free`/`realloc`, with a
//! side table tracking allocation sizes since Rust's allocator requires layout on deallocation.
//! Growth is handled automatically by the allocator (wee_alloc/dlmalloc will call `memory.grow`).

use std::alloc::{GlobalAlloc, Layout, System};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use js_sys::{Uint8Array, WebAssembly};
use lpvm::{AllocError, LpvmBuffer, LpvmMemory};
use wasm_bindgen::JsCast;

use crate::error::WasmError;
use crate::module::EnvMemorySpec;

/// Shared runtime state: the host WebAssembly.Memory handle.
///
/// This is the memory that shaders import as `env.memory`.
pub(crate) struct BrowserLpvmSharedRuntime {
    pub memory: WebAssembly::Memory,
}

impl BrowserLpvmSharedRuntime {
    pub(crate) fn new() -> Result<Arc<Self>, WasmError> {
        let memory = wasm_bindgen::memory()
            .dyn_into::<WebAssembly::Memory>()
            .map_err(|_| {
                WasmError::runtime(
                    "wasm_bindgen::memory() is not a WebAssembly.Memory; lpvm-wasm browser runtime requires wasm-bindgen",
                )
            })?;

        // Ensure minimum size for guest reserve
        Self::ensure_minimum_size(&memory)?;

        Ok(Arc::new(Self { memory }))
    }

    fn ensure_minimum_size(memory: &WebAssembly::Memory) -> Result<(), WasmError> {
        let engine_spec = EnvMemorySpec::engine_initial_for_host();
        let min_bytes = (engine_spec.initial_pages as usize)
            .saturating_mul(EnvMemorySpec::WASM_PAGE_SIZE as usize);

        let page = usize::try_from(EnvMemorySpec::WASM_PAGE_SIZE)
            .map_err(|_| WasmError::runtime("WASM page size"))?;

        let mut len = memory_byte_len(memory)
            .map_err(|_| WasmError::runtime("could not read wasm linear memory size"))?;

        while len < min_bytes {
            let need = min_bytes - len;
            let delta_pages = (need + page - 1) / page;
            let delta_u32 = u32::try_from(delta_pages)
                .map_err(|_| WasmError::runtime("memory grow page count"))?;
            let old_len = len;
            memory.grow(delta_u32);
            len = memory_byte_len(memory).map_err(|_| {
                WasmError::runtime("could not read wasm linear memory size after grow")
            })?;
            if len <= old_len {
                return Err(WasmError::runtime(
                    "failed to grow wasm linear memory to engine_initial_for_host size",
                ));
            }
        }

        Ok(())
    }
}

fn memory_byte_len(mem: &WebAssembly::Memory) -> Result<usize, WasmError> {
    let buf = mem.buffer();
    let a = Uint8Array::new(&buf);
    usize::try_from(a.length()).map_err(|_| WasmError::runtime("memory length overflow"))
}

/// Browser-side [`LpvmMemory`] using the Rust global allocator.
///
/// Uses a side table to track allocation sizes because Rust's allocator requires
/// the original `Layout` for deallocation.
pub(crate) struct BrowserLpvmMemory {
    /// Tracks allocation metadata: guest base offset → (size, align)
    allocations: Mutex<HashMap<u64, (usize, usize)>>,
}

impl BrowserLpvmMemory {
    pub(crate) fn new() -> Self {
        Self {
            allocations: Mutex::new(HashMap::new()),
        }
    }
}

impl LpvmMemory for BrowserLpvmMemory {
    fn alloc(&self, size: usize, align: usize) -> Result<LpvmBuffer, AllocError> {
        if size == 0 {
            return Err(AllocError::InvalidSize);
        }
        if !align.is_power_of_two() {
            return Err(AllocError::InvalidSize);
        }

        let layout = Layout::from_size_align(size, align).map_err(|_| AllocError::InvalidSize)?;

        // SAFETY: layout is valid (non-zero size, power-of-two alignment)
        let native = unsafe { System.alloc(layout) };
        if native.is_null() {
            return Err(AllocError::OutOfMemory);
        }

        let guest = native as usize as u64;

        // Track this allocation for deallocation
        self.allocations
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(guest, (size, align));

        Ok(LpvmBuffer::new(native, guest, size, align))
    }

    fn free(&self, buffer: LpvmBuffer) {
        let guest = buffer.guest_base();
        let size = buffer.size();
        let align = buffer.align();

        // Remove from tracking table
        let removed = self
            .allocations
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(&guest);

        if removed.is_none() {
            // Double-free or invalid buffer - leak it safely
            return;
        }

        let layout = Layout::from_size_align(size, align).expect("valid layout");

        // SAFETY: pointer came from our alloc, layout matches what we used
        unsafe {
            System.dealloc(buffer.native_ptr(), layout);
        }
    }

    fn realloc(&self, buffer: LpvmBuffer, new_size: usize) -> Result<LpvmBuffer, AllocError> {
        if new_size == 0 {
            self.free(buffer);
            return Err(AllocError::InvalidSize);
        }

        let guest = buffer.guest_base();
        let old_size = buffer.size();
        let align = buffer.align();

        // Look up the allocation
        let mut allocations = self
            .allocations
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        if !allocations.contains_key(&guest) {
            return Err(AllocError::InvalidPointer);
        }

        let old_layout =
            Layout::from_size_align(old_size, align).map_err(|_| AllocError::InvalidSize)?;
        let new_layout =
            Layout::from_size_align(new_size, align).map_err(|_| AllocError::InvalidSize)?;

        // SAFETY: pointer and layout came from our alloc
        let new_native =
            unsafe { System.realloc(buffer.native_ptr(), old_layout, new_layout.size()) };
        if new_native.is_null() {
            return Err(AllocError::OutOfMemory);
        }

        // Update tracking: remove old, insert new
        allocations.remove(&guest);
        let new_guest = new_native as usize as u64;
        allocations.insert(new_guest, (new_size, align));

        Ok(LpvmBuffer::new(new_native, new_guest, new_size, align))
    }
}
