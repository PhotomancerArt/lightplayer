//! Host linear allocation for [`LpvmMemory`] on the native JIT path (`std`).
//!
//! Guest and host share one address space: [`LpvmBuffer::guest_base`] equals the zero-extended
//! host pointer. A [`std::sync::Mutex`] + [`BTreeMap`] tracks live allocations so `free`/`realloc`
//! only accept buffers this allocator created (Rust's global `dealloc` needs the exact layout).

use alloc::alloc::{alloc, dealloc, realloc};
use alloc::collections::BTreeMap;

use core::alloc::Layout;

use lpvm::{AllocError, LpvmBuffer, LpvmMemory};
use std::sync::Mutex;

/// Real `alloc` / `dealloc` / `realloc` over the process heap for Cranelift JIT hosts.
pub struct CraneliftHostMemory {
    live: Mutex<BTreeMap<u64, (usize, usize)>>,
}

impl CraneliftHostMemory {
    pub fn new() -> Self {
        Self {
            live: Mutex::new(BTreeMap::new()),
        }
    }

    fn key(ptr: *mut u8) -> u64 {
        ptr as usize as u64
    }
}

impl Default for CraneliftHostMemory {
    fn default() -> Self {
        Self::new()
    }
}

impl LpvmMemory for CraneliftHostMemory {
    fn alloc(&self, size: usize, align: usize) -> Result<LpvmBuffer, AllocError> {
        if size == 0 {
            return Err(AllocError::InvalidSize);
        }
        if !align.is_power_of_two() {
            return Err(AllocError::InvalidSize);
        }
        let layout = Layout::from_size_align(size, align).map_err(|_| AllocError::InvalidSize)?;
        // SAFETY: layout validated
        let native = unsafe { alloc(layout) };
        if native.is_null() {
            return Err(AllocError::OutOfMemory);
        }
        let guest = Self::key(native);
        self.live
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(guest, (size, align));
        Ok(LpvmBuffer::new(native, guest, size, align))
    }

    fn free(&self, buffer: LpvmBuffer) {
        let guest = buffer.guest_base();
        let size = buffer.size();
        let align = buffer.align();
        let removed = self
            .live
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(&guest);
        if removed.is_none() {
            return;
        }
        let layout = match Layout::from_size_align(size, align) {
            Ok(l) => l,
            Err(_) => return,
        };
        // SAFETY: pointer and layout from our alloc
        unsafe {
            dealloc(buffer.native_ptr(), layout);
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

        let mut map = self
            .live
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if !map.contains_key(&guest) {
            return Err(AllocError::InvalidPointer);
        }
        let old_layout =
            Layout::from_size_align(old_size, align).map_err(|_| AllocError::InvalidSize)?;
        let new_layout =
            Layout::from_size_align(new_size, align).map_err(|_| AllocError::InvalidSize)?;

        // SAFETY: old ptr/layout from our alloc
        let new_native = unsafe { realloc(buffer.native_ptr(), old_layout, new_layout.size()) };
        if new_native.is_null() {
            return Err(AllocError::OutOfMemory);
        }
        map.remove(&guest);
        let new_guest = Self::key(new_native);
        map.insert(new_guest, (new_size, align));
        Ok(LpvmBuffer::new(new_native, new_guest, new_size, align))
    }
}
