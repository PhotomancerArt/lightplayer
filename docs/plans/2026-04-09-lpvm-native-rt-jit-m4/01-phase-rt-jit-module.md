# Phase 1: Create rt_jit Module Structure and JitBuffer

## Scope

Create the `rt_jit` module structure and the `JitBuffer` type for executable memory management.

## Implementation Details

### 1. Create `lpvm-native/src/rt_jit/mod.rs`

```rust
//! JIT compilation for RISC-V targets (no_std + alloc).
//!
//! This module provides direct JIT buffer compilation, bypassing ELF emission.
//! Only available on RISC-V targets.

#![cfg(target_arch = "riscv32")]

pub mod buffer;
pub mod builtins;
pub mod compiler;
pub mod engine;
pub mod module;
pub mod instance;

pub use buffer::JitBuffer;
pub use builtins::BuiltinTable;
pub use compiler::JitEmitContext;
pub use engine::NativeJitEngine;
pub use module::NativeJitModule;
pub use instance::NativeJitInstance;
```

### 2. Create `lpvm-native/src/rt_jit/buffer.rs`

```rust
//! Executable JIT buffer allocation and management.

use alloc::alloc::{alloc, dealloc};
use core::alloc::Layout;

/// Executable memory buffer for JIT code.
///
/// On ESP32-C6, DRAM is executable by default, so simple heap allocation works.
pub struct JitBuffer {
    ptr: *mut u8,
    len: usize,
    capacity: usize,
}

impl JitBuffer {
    /// Allocate a new buffer with given capacity.
    ///
    /// Returns Err if allocation fails.
    pub fn with_capacity(capacity: usize) -> Result<Self, ()> {
        let align = 4; // RISC-V instructions must be 4-byte aligned
        let layout = Layout::from_size_align(capacity, align).map_err(|_| ())?;
        let ptr = unsafe { alloc(layout) };
        if ptr.is_null() {
            return Err(());
        }
        Ok(Self {
            ptr,
            len: 0,
            capacity,
        })
    }

    /// Push bytes to the buffer.
    ///
    /// Panics if buffer is full (should not happen with proper sizing).
    pub fn push_bytes(&mut self, bytes: &[u8]) {
        assert!(self.len + bytes.len() <= self.capacity);
        unsafe {
            core::ptr::copy_nonoverlapping(
                bytes.as_ptr(),
                self.ptr.add(self.len),
                bytes.len(),
            );
        }
        self.len += bytes.len();
    }

    /// Push a single u32 (RISC-V instruction).
    pub fn push_u32(&mut self, word: u32) {
        self.push_bytes(&word.to_le_bytes());
    }

    /// Get entry pointer at given byte offset.
    ///
    /// # Safety
    /// - offset must be 4-byte aligned
    /// - offset must be within buffer bounds
    /// - buffer must contain valid RISC-V code
    pub unsafe fn entry_ptr(&self, offset: usize) -> *const u8 {
        assert!(offset < self.len);
        assert!(offset % 4 == 0);
        self.ptr.add(offset)
    }

    /// Get mutable pointer to code (for relocation patching).
    pub fn as_mut_ptr(&mut self) -> *mut u8 {
        self.ptr
    }

    /// Current code length in bytes.
    pub fn len(&self) -> usize {
        self.len
    }

    /// Buffer capacity in bytes.
    pub fn capacity(&self) -> usize {
        self.capacity
    }
}

impl Drop for JitBuffer {
    fn drop(&mut self) {
        if !self.ptr.is_null() {
            let layout = Layout::from_size_align(self.capacity, 4).unwrap();
            unsafe { dealloc(self.ptr, layout) };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buffer_allocation() {
        let buf = JitBuffer::with_capacity(1024).expect("allocate");
        assert_eq!(buf.len(), 0);
        assert_eq!(buf.capacity(), 1024);
    }

    #[test]
    fn push_u32() {
        let mut buf = JitBuffer::with_capacity(16).expect("allocate");
        buf.push_u32(0x12345678);
        assert_eq!(buf.len(), 4);
    }
}
```

### 3. Update `lpvm-native/src/lib.rs`

Add the `rt_jit` module (gated on RISC-V target):

```rust
// ... existing modules ...

#[cfg(target_arch = "riscv32")]
pub mod rt_jit;
```

## Validate

```bash
# Check that it compiles on RISC-V target
cargo check -p lpvm-native --target riscv32imac-unknown-none-elf

# Host build should still work (rt_jit excluded)
cargo check -p lpvm-native
```

## Code Organization

- Place helper functions at bottom of files
- Keep `JitBuffer` simple - it's just a growable executable buffer
- `Drop` implementation ensures cleanup
