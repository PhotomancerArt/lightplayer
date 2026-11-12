# Phase 1: Syscall + Guest TrackingAllocator

## Scope

Add the `SYSCALL_ALLOC_TRACE` constant to the shared crate and implement the
`TrackingAllocator` wrapper in `lp-riscv-emu-guest`, gated behind an
`alloc-trace` cargo feature. Guest-side only -- no host handling yet.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**.
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

## Implementation Details

### 1. Add syscall constant

In `lp-riscv/lp-riscv-emu-shared/src/syscall.rs`, add:

```rust
/// Syscall number for allocation tracing (alloc/dealloc/realloc events)
pub const SYSCALL_ALLOC_TRACE: i32 = 9;
```

### 2. Add `alloc-trace` feature to `lp-riscv-emu-guest`

In `lp-riscv/lp-riscv-emu-guest/Cargo.toml`:

```toml
[features]
alloc-trace = []
```

### 3. Implement TrackingAllocator

In `lp-riscv/lp-riscv-emu-guest/src/allocator.rs`:

The key design constraints:
- Must implement `GlobalAlloc` (unsafe trait)
- All code paths must be **allocation-free** (we hold the allocator lock)
- The syscall uses only registers (stack + ecall), no heap
- `Heap::free()` is called while we still hold the lock via the inner allocator

When `alloc-trace` is **disabled**, the module is unchanged -- plain `LockedHeap`.

When `alloc-trace` is **enabled**, the `#[global_allocator]` becomes a
`TrackingAllocator` that wraps `LockedHeap`.

```rust
#[cfg(not(feature = "alloc-trace"))]
#[global_allocator]
static HEAP_ALLOCATOR: LockedHeap = LockedHeap::empty();

#[cfg(feature = "alloc-trace")]
#[global_allocator]
static HEAP_ALLOCATOR: TrackingAllocator = TrackingAllocator::new();
```

The `TrackingAllocator` struct:

```rust
#[cfg(feature = "alloc-trace")]
pub struct TrackingAllocator {
    inner: LockedHeap,
}
```

Allocation event types (passed as syscall arg):

```rust
const ALLOC_EVENT: i32 = 0;
const DEALLOC_EVENT: i32 = 1;
const REALLOC_EVENT: i32 = 2;
```

For `alloc`:
- Call `self.inner.alloc(layout)` to get the pointer
- If non-null, get free bytes from the inner heap
- Syscall: `a0=ALLOC_EVENT, a1=ptr, a2=size, a3=free_bytes`

For `dealloc`:
- Syscall first (before freeing, so free_bytes reflects pre-dealloc state)
- Actually: syscall after dealloc is better -- free_bytes shows post-dealloc state
  which is more useful for tracking. Let's do: dealloc first, then read free, then syscall.
- Call `self.inner.dealloc(ptr, layout)`
- Read free bytes
- Syscall: `a0=DEALLOC_EVENT, a1=ptr, a2=size, a3=free_bytes`

For `realloc`:
- Call `self.inner.realloc(old_ptr, layout, new_size)` to get new_ptr
- Read free bytes
- Syscall: `a0=REALLOC_EVENT, a1=old_ptr, a2=new_ptr, a3=old_size, a4=new_size, a5=free_bytes`

**Accessing `Heap::free()`**: `LockedHeap` is a `Mutex<Heap>`. The `GlobalAlloc`
impl on `LockedHeap` acquires the lock internally. We need to call `alloc` on
the inner `Heap` directly and then read `free()` while still holding the lock.

Looking at `linked_list_allocator` 0.10's API:
- `LockedHeap::lock()` returns a `MutexGuard<Heap>`
- `Heap::allocate_first_fit(layout)` returns `Result<NonNull<u8>, ()>`
- `Heap::deallocate(ptr, layout)`
- `Heap::free()` returns total free bytes

So the `GlobalAlloc` impl should lock once, do the operation + read free, then
drop the lock:

```rust
unsafe impl GlobalAlloc for TrackingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let (ptr, free) = {
            let mut heap = self.inner.lock();
            let ptr = heap
                .allocate_first_fit(layout)
                .ok()
                .map_or(core::ptr::null_mut(), |nn| nn.as_ptr());
            let free = heap.free();
            (ptr, free)
        };
        if !ptr.is_null() {
            self.trace_alloc_event(ALLOC_EVENT, ptr as i32, layout.size() as i32, free as i32);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let free = {
            let mut heap = self.inner.lock();
            heap.deallocate(core::ptr::NonNull::new_unchecked(ptr), layout);
            heap.free()
        };
        self.trace_alloc_event(DEALLOC_EVENT, ptr as i32, layout.size() as i32, free as i32);
    }
}
```

The `trace_alloc_event` helper makes the syscall:

```rust
#[inline(never)]
fn trace_alloc_event(&self, event_type: i32, ptr: i32, size: i32, free: i32) {
    let mut args = [0i32; SYSCALL_ARGS];
    args[0] = event_type;
    args[1] = ptr;
    args[2] = size;
    args[3] = free;
    crate::syscall::syscall(SYSCALL_ALLOC_TRACE, &args);
}
```

For `realloc`, implement similarly with 6 args (type, old_ptr, new_ptr, old_sz,
new_sz, free). Override the default `GlobalAlloc::realloc` to avoid the
default alloc+copy+dealloc path generating two separate events.

### 4. Update `init_heap()`

The `init_heap()` function needs to work with both the plain and tracking
allocator:

```rust
pub unsafe fn init_heap() {
    // ... same linker symbol extraction ...
    #[cfg(not(feature = "alloc-trace"))]
    unsafe {
        HEAP_ALLOCATOR.lock().init(heap_start, heap_size);
    }
    #[cfg(feature = "alloc-trace")]
    unsafe {
        HEAP_ALLOCATOR.inner.lock().init(heap_start, heap_size);
    }
}
```

### 5. Re-export syscall constant

In `lp-riscv/lp-riscv-emu-guest/src/syscall.rs`, add `SYSCALL_ALLOC_TRACE` to
the re-exports from `lp_riscv_emu_shared`.

## Validate

```bash
# Check that guest compiles without the feature (no change in behavior)
cargo check -p lp-riscv-emu-guest --target riscv32imac-unknown-none-elf

# Check that guest compiles with the feature
cargo check -p lp-riscv-emu-guest --target riscv32imac-unknown-none-elf --features alloc-trace

# Check shared crate
cargo check -p lp-riscv-emu-shared

# Run existing tests still pass
cargo test -p lp-riscv-emu-shared
```

Note: `lp-riscv-emu-guest` is a `no_std` crate targeting `riscv32imac-unknown-none-elf`,
so `cargo test` won't work for it directly. Compilation check is sufficient here.
Full integration testing happens in Phase 3.
