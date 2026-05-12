# Phase 1: ShaderPtr and AllocError Types

## Scope

Create `ShaderPtr` struct and `AllocError` enum. These are the fundamental
data types for shared memory support.

## Code Organization Reminders

- Place entry points and public types first in files
- Place helper utility functions at the bottom
- Use clear module documentation comments
- Mark `unsafe` operations explicitly

## Implementation Details

### New file: `lp-shader/lpvm/src/shader_ptr.rs`

```rust
//! Shared memory pointer with dual native/guest representation.
//!
//! `ShaderPtr` pairs a host-accessible pointer with a guest-visible value.
//! The host uses `native_ptr()` for direct memory access (unsafe). The guest
//! receives the `guest_value()` through uniforms and uses it for Load/Store
//! operations.
//!
//! # Safety
//!
//! `native_ptr()` returns a raw pointer. Using it is `unsafe` and requires
//! synchronization with shader execution. The shared memory is inherently
//! shared-mutable; concurrent access must be coordinated.

use core::fmt;

/// Pointer to shared memory accessible by both host and guest.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShaderPtr {
    native: *mut u8,
    guest: u64,
}

impl ShaderPtr {
    /// Create a new ShaderPtr with the given native and guest values.
    ///
    /// # Safety
    ///
    /// The `native` pointer must be valid for the lifetime of the allocation
    /// and properly aligned for access. The `guest` value must be the correct
    /// representation for the target backend.
    pub unsafe fn new(native: *mut u8, guest: u64) -> Self {
        Self { native, guest }
    }

    /// Get the native host pointer.
    ///
    /// # Safety
    ///
    /// Dereferencing this pointer is unsafe. The memory it points to is
    /// shared-mutable and may be accessed by shader instances concurrently.
    /// Host code must synchronize access.
    pub fn native_ptr(&self) -> *mut u8 {
        self.native
    }

    /// Get the guest-visible value.
    ///
    /// This value is passed to shaders through uniforms. The shader uses it
    /// as the base for Load/Store operations.
    ///
    /// For 32-bit targets (WASM, RV32, ESP32), this is the lower 32 bits.
    /// For 64-bit JIT targets, this is the full 64-bit pointer value.
    pub fn guest_value(&self) -> u64 {
        self.guest
    }
}

// SAFETY: ShaderPtr contains a raw pointer but is safe to Send/Sync because
// the pointer is just a value. The actual memory access safety is the caller's
// responsibility (marked unsafe in native_ptr()).
unsafe impl Send for ShaderPtr {}
unsafe impl Sync for ShaderPtr {}
```

### New file: `lp-shader/lpvm/src/memory.rs`

```rust
//! Shared memory allocation trait and error types.

use crate::ShaderPtr;
use core::fmt;

/// Errors that can occur during memory allocation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AllocError {
    /// Allocation failed: out of memory or address space.
    OutOfMemory,
    /// Invalid size parameter (zero or overflow).
    InvalidSize,
    /// Invalid pointer for free/realloc.
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

/// Trait for shared memory allocators.
///
/// This trait is object-safe (`dyn LpvmMemory`) for use in trait objects.
/// Implementations use interior mutability (atomics, RefCell, etc.) to
/// allow `&self` methods.
///
/// # Safety
///
/// Allocated memory is shared-mutable. The host accesses it via
/// `ShaderPtr::native_ptr()` (unsafe). Shaders access it through
/// uniforms containing `ShaderPtr::guest_value()`. Implementations
/// must ensure thread safety.
pub trait LpvmMemory {
    /// Allocate `size` bytes of shared memory.
    ///
    /// Returns a `ShaderPtr` with both native and guest representations.
    /// The native pointer is valid for the lifetime of the allocation
    /// (until `free()` is called).
    fn alloc(&self, size: usize) -> Result<ShaderPtr, AllocError>;

    /// Free memory allocated by `alloc()`.
    fn free(&self, ptr: ShaderPtr);

    /// Resize an allocation.
    ///
    /// If successful, returns a new `ShaderPtr` (may be different from
    /// the input). The old pointer is invalid after a successful realloc.
    /// On failure, the original allocation remains valid.
    fn realloc(&self, ptr: ShaderPtr, new_size: usize) -> Result<ShaderPtr, AllocError>;
}
```

## Tests

Add unit tests in `lp-shader/lpvm/src/shader_ptr.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;

    #[test]
    fn shader_ptr_roundtrip() {
        let native = Vec::leak(vec![0u8; 16]).as_mut_ptr();
        let guest: u64 = 0x80000000;
        let ptr = unsafe { ShaderPtr::new(native, guest) };

        assert_eq!(ptr.native_ptr(), native);
        assert_eq!(ptr.guest_value(), guest);
    }

    #[test]
    fn shader_ptr_is_copy() {
        let native = Vec::leak(vec![0u8; 4]).as_mut_ptr();
        let ptr = unsafe { ShaderPtr::new(native, 0) };
        let ptr2 = ptr;
        let ptr3 = ptr;

        assert_eq!(ptr.native_ptr(), ptr2.native_ptr());
        assert_eq!(ptr.guest_value(), ptr3.guest_value());
    }
}
```

Add tests in `lp-shader/lpvm/src/memory.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alloc_error_display() {
        assert_eq!(AllocError::OutOfMemory.to_string(), "out of memory");
        assert_eq!(AllocError::InvalidSize.to_string(), "invalid allocation size");
        assert_eq!(AllocError::InvalidPointer.to_string(), "invalid pointer");
    }
}
```

## Validate

```bash
cargo check -p lpvm
cargo test -p lpvm shader_ptr
cargo test -p lpvm alloc_error
```
