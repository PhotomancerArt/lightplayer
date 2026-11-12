## Phase 2: Create lpvm-emu Crate Skeleton and EmuMemory

Create the new crate structure and implement `LpvmMemory` trait for emulator shared memory.

### Code Organization

**File: `lp-shader/lpvm-emu/Cargo.toml`**

```toml
[package]
name = "lpvm-emu"
version.workspace = true
edition.workspace = true
license.workspace = true
description = "LPVM trait implementation for RV32 emulator"

[lints]
workspace = true

[features]
default = ["std"]
std = ["lpvm/std", "lp-riscv-emu/std"]

[dependencies]
lpvm = { path = "../lpvm", default-features = false }
lp-riscv-emu = { path = "../../lp-riscv/lp-riscv-emu", default-features = false }
lpvm-cranelift = { path = "../lpvm-cranelift", default-features = false }
lpir = { path = "../lpir" }
lps-shared = { path = "../lps-shared" }

# For shared memory Arc/Mutex
parking_lot = { version = "1.5", optional = true }

[dev-dependencies]
# For tests
```

**File: `lp-shader/lpvm-emu/src/lib.rs`**

```rust
//! LPVM trait implementation for RV32 emulator (`lp-riscv-emu`).
//!
//! Provides `EmuEngine`, `EmuModule`, and `EmuInstance` implementing the
//! LPVM traits for running compiled RV32 code in the emulator.

#![no_std]

#[cfg(feature = "std")]
extern crate std;

extern crate alloc;

mod compile;
mod engine;
mod instance;
mod memory;
mod module;

pub use engine::EmuEngine;
pub use instance::EmuInstance;
pub use memory::EmuMemory;
pub use module::EmuModule;

#[cfg(test)]
mod tests {
    use super::*;
    use lpvm::{LpvmEngine, LpvmMemory};
    
    // Basic instantiation test
    #[test]
    fn engine_creates_default() {
        let engine = EmuEngine::new(Default::default());
        assert!(engine.memory().alloc(32, 8).is_ok());
    }
}
```

**File: `lp-shader/lpvm-emu/src/memory.rs`**

Implements `LpvmMemory` using a bump allocator within a shared memory Vec.

```rust
use alloc::vec::Vec;
use core::sync::atomic::{AtomicUsize, Ordering};
use lpvm::{AllocError, LpvmBuffer, LpvmMemory, LpvmPtr};

/// Default shared memory size (256KB matching BumpLpvmMemory).
pub const DEFAULT_SHARED_BYTES: usize = 256 * 1024;

/// Bump allocator for emulator shared memory.
///
/// Allocations are sequential with no reuse. This is sufficient for
/// shader execution where allocations happen at setup time.
pub struct EmuMemory {
    buffer: Vec<u8>,
    next_offset: AtomicUsize,
}

impl EmuMemory {
    pub fn new() -> Self {
        Self::with_capacity(DEFAULT_SHARED_BYTES)
    }
    
    pub fn with_capacity(bytes: usize) -> Self {
        Self {
            buffer: vec![0u8; bytes],
            next_offset: AtomicUsize::new(0),
        }
    }
    
    /// Get a reference to the underlying buffer (for emulator setup).
    pub fn buffer(&self) -> &Vec<u8> {
        &self.buffer
    }
}

impl LpvmMemory for EmuMemory {
    fn alloc(&self, size: usize, align: usize) -> Result<LpvmBuffer, AllocError> {
        let align_mask = align - 1;
        let current = self.next_offset.load(Ordering::Relaxed);
        let aligned = (current + align_mask) & !align_mask;
        let end = aligned + size;
        
        if end > self.buffer.len() {
            return Err(AllocError::OutOfMemory);
        }
        
        // Try to claim this space
        match self.next_offset.compare_exchange(
            current,
            end,
            Ordering::Relaxed,
            Ordering::Relaxed,
        ) {
            Ok(_) => {
                let ptr = unsafe { self.buffer.as_ptr().add(aligned) };
                Ok(LpvmBuffer::new(
                    ptr,
                    size,
                    align,
                    (ptr as usize) as u64,  // guest_base = host ptr (same address space)
                ))
            }
            Err(_) => {
                // Another thread claimed space, retry
                self.alloc(size, align)
            }
        }
    }
    
    fn free(&self, _buffer: LpvmBuffer) {
        // Bump allocator doesn't support free
        // Could add ref counting or track for reset
    }
    
    fn realloc(&self, buffer: LpvmBuffer, new_size: usize) -> Result<LpvmBuffer, AllocError> {
        if new_size <= buffer.size() {
            // Shrink - return same buffer with new size
            return Ok(LpvmBuffer::new(
                buffer.native_ptr(),
                new_size,
                buffer.align(),
                buffer.guest_base(),
            ));
        }
        
        // Grow: check if this was the last allocation
        let current_end = buffer.native_ptr() as usize + buffer.size();
        let current_next = self.next_offset.load(Ordering::Relaxed);
        
        if current_end == current_next {
            // This was the last allocation, try to grow in place
            let additional = new_size - buffer.size();
            let new_end = current_next + additional;
            
            if new_end > self.buffer.len() {
                return Err(AllocError::OutOfMemory);
            }
            
            match self.next_offset.compare_exchange(
                current_next,
                new_end,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => {
                    return Ok(LpvmBuffer::new(
                        buffer.native_ptr(),
                        new_size,
                        buffer.align(),
                        buffer.guest_base(),
                    ));
                }
                Err(_) => {}
            }
        }
        
        // Fallback: allocate new and copy
        let new = self.alloc(new_size, buffer.align())?;
        unsafe {
            core::ptr::copy_nonoverlapping(
                buffer.native_ptr(),
                new.native_ptr(),
                buffer.size().min(new_size),
            );
        }
        Ok(new)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpvm::LpvmMemory;
    
    #[test]
    fn alloc_returns_valid_buffer() {
        let mem = EmuMemory::new();
        let buf = mem.alloc(64, 8).unwrap();
        assert_eq!(buf.size(), 64);
        assert_eq!(buf.align(), 8);
        assert!(!buf.native_ptr().is_null());
    }
    
    #[test]
    fn guest_base_matches_host_ptr() {
        let mem = EmuMemory::new();
        let buf = mem.alloc(32, 4).unwrap();
        assert_eq!(buf.guest_base(), buf.native_ptr() as usize as u64);
    }
    
    #[test]
    fn alignment_respected() {
        let mem = EmuMemory::new();
        let _ = mem.alloc(10, 1).unwrap();  // 1-byte aligned
        let buf2 = mem.alloc(64, 16).unwrap();  // 16-byte aligned
        assert_eq!(buf2.native_ptr() as usize % 16, 0);
    }
    
    #[test]
    fn out_of_memory_detected() {
        let mem = EmuMemory::with_capacity(64);
        assert!(mem.alloc(128, 8).is_err());  // Too big
        assert!(mem.alloc(64, 8).is_ok());     // Exact fit
        assert!(mem.alloc(1, 1).is_err());   // No space left
    }
    
    #[test]
    fn realloc_grow_in_place() {
        let mem = EmuMemory::new();
        let buf = mem.alloc(32, 8).unwrap();
        let ptr = buf.native_ptr();
        let guest = buf.guest_base();
        
        let new = mem.realloc(buf, 64).unwrap();
        
        // In-place realloc should preserve pointer
        assert_eq!(new.native_ptr(), ptr);
        assert_eq!(new.guest_base(), guest);
        assert_eq!(new.size(), 64);
    }
}
```

### Validate

```bash
cargo check -p lpvm-emu
cargo check -p lpvm-emu --no-default-features
cargo test -p lpvm-emu
```

Check that `LpvmMemory` trait is properly implemented (compiler will verify).
