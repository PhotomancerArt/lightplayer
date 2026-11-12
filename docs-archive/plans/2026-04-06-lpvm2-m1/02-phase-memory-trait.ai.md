# Phase 2: LpvmMemory Trait Implementation Pattern

## Scope

This phase documents the implementation pattern for `LpvmMemory` backends.
No new code files are created — the pattern is for backend implementors.

## Code Organization Reminders

- Interior mutability belongs in the implementation, not the trait
- Document the synchronization strategy for each backend
- Test with concurrent access patterns

## Implementation Pattern

### Interior Mutability Strategy

Since `LpvmMemory` methods take `&self`, implementations use interior mutability:

**WASM (wasmtime)**:
```rust
use core::sync::atomic::{AtomicUsize, Ordering};

pub struct WasmMemory {
    memory: Memory,           // wasmtime::Memory
    base: *mut u8,            // Memory base pointer (valid for lifetime)
    bump: AtomicUsize,        // Next free offset
}

impl LpvmMemory for WasmMemory {
    fn alloc(&self, size: usize) -> Result<ShaderPtr, AllocError> {
        let offset = self.bump.fetch_add(size, Ordering::SeqCst);
        if offset + size > self.memory.size() {
            // Try memory.grow() or fail
        }
        let native = unsafe { self.base.add(offset) };
        let guest = offset as u64;
        unsafe { Ok(ShaderPtr::new(native, guest)) }
    }
    // ...
}
```

**Cranelift JIT (32-bit)**:
```rust
use core::cell::RefCell;

pub struct JitMemory32 {
    heap: RefCell<Vec<u8>>,   // Actual allocation buffer
    bump: AtomicUsize,        // Allocation cursor
}

impl LpvmMemory for JitMemory32 {
    fn alloc(&self, size: usize) -> Result<ShaderPtr, AllocError> {
        let mut heap = self.heap.borrow_mut();
        let offset = self.bump.fetch_add(size, Ordering::SeqCst);
        if offset + size > heap.len() {
            heap.resize(offset + size, 0);
        }
        let native = heap.as_mut_ptr().wrapping_add(offset);
        let guest = offset as u64;  // Same as native for 32-bit
        unsafe { Ok(ShaderPtr::new(native, guest)) }
    }
    // ...
}
```

**Cranelift JIT (64-bit)**:
```rust
pub struct JitMemory64;

impl LpvmMemory for JitMemory64 {
    fn alloc(&self, size: usize) -> Result<ShaderPtr, AllocError> {
        let layout = Layout::from_size_align(size, 16).map_err(|_| AllocError::InvalidSize)?;
        let ptr = unsafe { alloc::alloc::alloc(layout) };
        if ptr.is_null() {
            return Err(AllocError::OutOfMemory);
        }
        let guest = ptr as u64;  // Full 64-bit pointer
        unsafe { Ok(ShaderPtr::new(ptr, guest)) }
    }
    // ...
}
```

### Thread Safety Notes

- **WASM**: `Memory` is inherently thread-safe (host control)
- **JIT (32-bit)**: `RefCell` or `RwLock` for heap resizing, atomics for bump
- **JIT (64-bit)**: System allocator is thread-safe
- **Emulator**: Filetests are single-threaded, simple RefCell sufficient

## Validate

No validation in this phase — it's documentation for future backend work.

```bash
cargo check -p lpvm  # Trait compiles, no errors
```
