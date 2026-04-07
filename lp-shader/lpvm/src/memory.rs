//! Shared memory allocator trait and a small bump allocator for hosts/tests.

use core::fmt;
use core::sync::atomic::{AtomicUsize, Ordering};

use crate::ShaderPtr;

/// Fixed alignment for bump allocations (reasonable for texture rows, structs).
const BUMP_ALIGN: usize = 16;

fn round_up(value: usize, align: usize) -> usize {
    debug_assert!(align.is_power_of_two());
    let mask = align - 1;
    match value.checked_add(mask) {
        Some(v) => v & !mask,
        None => usize::MAX,
    }
}

/// Allocation failure independent of backend `LpvmEngine::Error`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AllocError {
    /// No space left or backend cannot satisfy the request.
    OutOfMemory,
    /// Zero size or arithmetic overflow.
    InvalidSize,
    /// `free` / `realloc` on an unknown pointer (not tracked by this allocator).
    InvalidPointer,
}

impl fmt::Display for AllocError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AllocError::OutOfMemory => write!(f, "out of memory"),
            AllocError::InvalidSize => write!(f, "invalid allocation size"),
            AllocError::InvalidPointer => write!(f, "invalid pointer"),
        }
    }
}

impl core::error::Error for AllocError {}

/// Object-safe shared memory API (`dyn LpvmMemory`).
///
/// Implementations use interior mutability. Hosts access bytes through
/// [`ShaderPtr::native_ptr`]; guests use [`ShaderPtr::guest_value`].
pub trait LpvmMemory {
    fn alloc(&self, size: usize) -> Result<ShaderPtr, AllocError>;
    fn free(&self, ptr: ShaderPtr);
    fn realloc(&self, ptr: ShaderPtr, new_size: usize) -> Result<ShaderPtr, AllocError>;
}

/// Bump allocator over a fixed host buffer; [`Sync`] for `&dyn LpvmMemory`.
///
/// Intended as a default host-side heap until backends wire real shared memory
/// (WASM linear memory, emulator region, etc.). `free` is a no-op; `realloc`
/// only succeeds for the most recent allocation at the end of the bump.
pub struct BumpLpvmMemory {
    storage: alloc::boxed::Box<[u8]>,
    next: AtomicUsize,
}

impl BumpLpvmMemory {
    /// Allocate an empty bump arena of `capacity` bytes (all zero).
    pub fn new(capacity: usize) -> Self {
        Self {
            storage: alloc::vec![0u8; capacity].into_boxed_slice(),
            next: AtomicUsize::new(0),
        }
    }

    fn len(&self) -> usize {
        self.storage.len()
    }

    fn base(&self) -> *mut u8 {
        self.storage.as_ptr() as *mut u8
    }
}

impl LpvmMemory for BumpLpvmMemory {
    fn alloc(&self, size: usize) -> Result<ShaderPtr, AllocError> {
        if size == 0 {
            return Err(AllocError::InvalidSize);
        }
        loop {
            let pos = self.next.load(Ordering::Relaxed);
            let aligned = round_up(pos, BUMP_ALIGN);
            let end = aligned.checked_add(size).ok_or(AllocError::InvalidSize)?;
            if end > self.len() {
                return Err(AllocError::OutOfMemory);
            }
            match self
                .next
                .compare_exchange_weak(pos, end, Ordering::SeqCst, Ordering::Relaxed)
            {
                Ok(_) => {
                    let native = unsafe { self.base().add(aligned) };
                    let guest = native as usize as u64;
                    return Ok(ShaderPtr::new(native, guest));
                }
                Err(_) => continue,
            }
        }
    }

    fn free(&self, _ptr: ShaderPtr) {
        // Bump allocator: leak semantics until a real free list exists.
    }

    fn realloc(&self, _ptr: ShaderPtr, _new_size: usize) -> Result<ShaderPtr, AllocError> {
        // BumpLpvmMemory does not record allocation sizes; use `alloc` + copy for resize.
        Err(AllocError::InvalidPointer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;

    #[test]
    fn alloc_error_display() {
        assert_eq!(AllocError::OutOfMemory.to_string(), "out of memory");
        assert_eq!(
            AllocError::InvalidSize.to_string(),
            "invalid allocation size"
        );
        assert_eq!(AllocError::InvalidPointer.to_string(), "invalid pointer");
    }

    #[test]
    fn bump_alloc_sequence() {
        let mem = BumpLpvmMemory::new(256);
        let a = mem.alloc(8).expect("alloc a");
        let b = mem.alloc(8).expect("alloc b");
        assert_ne!(a.native_ptr(), b.native_ptr());
        assert_eq!(unsafe { b.native_ptr().offset_from(a.native_ptr()) }, 16);
    }

    #[test]
    fn bump_out_of_memory() {
        let mem = BumpLpvmMemory::new(16);
        assert!(mem.alloc(32).is_err());
    }
}
