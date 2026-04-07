//! Shared memory allocator trait and a small bump allocator for hosts/tests.

use core::fmt;
use core::sync::atomic::{AtomicUsize, Ordering};

use crate::buffer::LpvmBuffer;

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
/// [`LpvmBuffer::native_ptr`]; guests use [`LpvmBuffer::guest_base`].
pub trait LpvmMemory {
    fn alloc(&self, size: usize, align: usize) -> Result<LpvmBuffer, AllocError>;
    fn free(&self, buffer: LpvmBuffer);
    fn realloc(&self, buffer: LpvmBuffer, new_size: usize) -> Result<LpvmBuffer, AllocError>;
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
    fn alloc(&self, size: usize, align: usize) -> Result<LpvmBuffer, AllocError> {
        if size == 0 {
            return Err(AllocError::InvalidSize);
        }
        if !align.is_power_of_two() {
            return Err(AllocError::InvalidSize);
        }
        loop {
            let pos = self.next.load(Ordering::Relaxed);
            let aligned = round_up(pos, align);
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
                    return Ok(LpvmBuffer::new(native, guest, size, align));
                }
                Err(_) => continue,
            }
        }
    }

    fn free(&self, _buffer: LpvmBuffer) {
        // Bump allocator: leak semantics until a real free list exists.
    }

    fn realloc(&self, buffer: LpvmBuffer, new_size: usize) -> Result<LpvmBuffer, AllocError> {
        // BumpLpvmMemory only supports realloc for the most recent allocation
        // at the end of the bump. For simplicity, we always fail here.
        // Users should alloc + copy + free old.
        let _ = (buffer, new_size);
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
        let a = mem.alloc(8, 8).expect("alloc a");
        let b = mem.alloc(8, 8).expect("alloc b");
        assert_ne!(a.native_ptr(), b.native_ptr());
        // With 8-byte alignment, 8-byte allocations are contiguous: 8 + 8 = 16
        assert_eq!(unsafe { b.native_ptr().offset_from(a.native_ptr()) }, 8);
    }

    #[test]
    fn bump_out_of_memory() {
        let mem = BumpLpvmMemory::new(16);
        assert!(mem.alloc(32, 8).is_err());
    }

    #[test]
    fn bump_zero_size_fails() {
        let mem = BumpLpvmMemory::new(256);
        assert!(matches!(mem.alloc(0, 8), Err(AllocError::InvalidSize)));
    }

    #[test]
    fn bump_invalid_align_fails() {
        let mem = BumpLpvmMemory::new(256);
        assert!(matches!(mem.alloc(16, 3), Err(AllocError::InvalidSize)));
    }
}
