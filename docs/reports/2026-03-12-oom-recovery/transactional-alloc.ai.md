# OOM Recovery: Transactional Allocator Design

**Date:** 2026-03-12
**Context:** ESP32 (320 KB heap) and fw-emu both face OOM during shader compilation due to Cranelift's allocation-heavy codegen. Fragmentation makes OOMs semi-random. We need a way to catch OOMs, reclaim all memory from the failed operation, and continue running in a degraded state.

---

## Goals

1. **Isolate OOMs to the operation that caused them.** A shader compilation failure should not crash the firmware.
2. **Reclaim all memory** allocated during the failed operation, so the system can continue.
3. **No ROM/flash cost.** No `.eh_frame`, no unwind tables, no landing pads. We need flash space for a web UI.
4. **Testable in the emulator.** The fw-emu guest runs the same allocator and engine code as ESP32.

## Non-Goals (for now)

- Automatic project reload after OOM (future layer on top)
- Automatic ESP32 reboot after OOM (future layer on top)
- Protecting arbitrary code paths — only well-understood, allocation-heavy operations (compilation)

---

## Approach: `setjmp`/`longjmp` + Transactional Allocator

### Why not stack unwinding?

Real unwinding (the `unwinding` crate) would let us use `catch_unwind` and run destructors normally. But it requires keeping `.eh_frame` sections in flash (currently discarded) and switching from `panic = "abort"` to `panic = "unwind"`, adding ~10-20% to binary size. With a web UI planned for flash, this cost is too high.

### Why not a bump arena?

A bump arena can't free individual allocations — dealloc is a no-op. Cranelift does heavy alloc/dealloc churn during compilation (builds IR, tears it down, builds regalloc state, etc.). With a bump arena, peak memory during compilation would be significantly higher because "freed" memory stays consumed. On 320 KB that makes things worse.

### The approach

Wrap the global allocator. During a protected "transaction", every allocation is tracked in a linked list embedded in allocation headers. Normal `dealloc` works as usual (frees memory and removes from the list). On OOM, `longjmp` back to the recovery point, then roll back the transaction: walk the tracking list and free everything that wasn't already freed.

---

## Allocator Design

### Allocation header

Every allocation made during an active transaction gets a header prepended:

```rust
#[repr(C)]
struct AllocHeader {
    prev: *mut AllocHeader,
    next: *mut AllocHeader,
    size: u32,
    align: u32,
}
// 16 bytes on RV32 (doubly-linked for O(1) unlink on dealloc)
```

### Transaction state

```rust
struct AllocTransaction {
    head: *mut AllocHeader,
    tail: *mut AllocHeader,
    count: u32,
    total_bytes: u32,
}
```

Stored in a `static mut` (single-threaded on both targets). Only one transaction can be active at a time.

### Global allocator wrapper

```rust
struct OomRecoverableAllocator<A: GlobalAlloc> {
    inner: A,
}

unsafe impl<A: GlobalAlloc> GlobalAlloc for OomRecoverableAllocator<A> {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if transaction_is_active() {
            // Allocate header + requested size
            let full_layout = layout_with_header(layout);
            let ptr = self.inner.alloc(full_layout);
            if ptr.is_null() {
                return ptr; // OOM — alloc_error_handler will longjmp
            }
            let header = ptr as *mut AllocHeader;
            (*header) = AllocHeader {
                prev: null_mut(),
                next: null_mut(),
                size: layout.size() as u32,
                align: layout.align() as u32,
            };
            transaction_link(header);
            // Return pointer past the header
            header.add(1) as *mut u8
        } else {
            self.inner.alloc(layout)
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if transaction_is_active() && transaction_owns(ptr) {
            let header = (ptr as *mut AllocHeader).sub(1);
            transaction_unlink(header); // O(1) doubly-linked remove
            let full_layout = layout_with_header(layout);
            self.inner.dealloc(header as *mut u8, full_layout);
        } else {
            self.inner.dealloc(ptr, layout);
        }
    }
}
```

### `transaction_owns` check

The `dealloc` path needs to distinguish transaction-tracked allocations from non-transaction allocations (made before the transaction started, or from statics). Two options:

**Option A: Pointer range check.** Impractical — transaction allocations aren't contiguous.

**Option B: Magic sentinel in the header.** Add a `magic: u32` field to `AllocHeader`. Before dealloc, check if `*(ptr - sizeof(AllocHeader)).magic == MAGIC`. If yes, it's tracked; unlink and free with header. If no, it's a normal allocation; free directly. Cost: 4 extra bytes per tracked allocation (20 bytes total header).

**Option C: Always prepend header when transaction is active.** Since we only activate transactions around specific operations, all allocations during that window get headers. The only `dealloc` calls during an active transaction that hit non-transaction allocations would be for objects allocated *before* the transaction started. We can detect this by address: if `ptr - sizeof(AllocHeader)` doesn't point to a valid linked-list node, it's not ours. The magic sentinel makes this safe.

Recommend **Option B** — simple, 4 bytes per allocation, no false positives.

The sentinel must be a value that is structurally impossible as a valid `prev` pointer. Since all allocator-returned pointers are at least 4-byte aligned, any odd value can never be a valid pointer. We use a recognizable odd value for easy identification in memory dumps and instruction logs:

```rust
const ALLOC_TXN_MAGIC: u32 = 0xA110_CA11; // "ALLOC ALL", odd → can never be a valid aligned pointer

#[repr(C)]
struct AllocHeader {
    magic: u32,
    prev: *mut AllocHeader,
    next: *mut AllocHeader,
    size: u32,
    align: u32,
}
// 20 bytes on RV32
```

On `dealloc`, the magic field is checked at `ptr - sizeof(AllocHeader)`. If it matches `ALLOC_TXN_MAGIC`, the allocation is transaction-tracked; otherwise it's a normal allocation passed straight to the inner allocator. The odd sentinel makes false positives structurally impossible, not merely improbable.

### Overhead

- **Per tracked allocation:** 20 bytes (header) + alignment padding to maintain the requested alignment. For 4-byte aligned allocations (most on RV32), the header is already 20 bytes = 5 words, so 4 bytes of padding to reach 24 bytes. Effective overhead: **24 bytes per allocation.**
- **Per non-tracked allocation (outside transaction):** Zero. The wrapper is a pass-through.
- **During Cranelift compilation:** If there are ~500 live allocations at peak, tracking overhead is ~12 KB. On a 320 KB heap this is significant but bounded, and only present during the transaction window.

---

## Recovery Flow

### `setjmp`/`longjmp` on RV32

No `libc` available in `no_std`. Implement in ~20 lines of assembly:

```rust
#[repr(C)]
struct JmpBuf {
    ra: u32,
    sp: u32,
    s: [u32; 12], // s0-s11
}
// 56 bytes

extern "C" {
    /// Returns 0 on direct call, non-zero on longjmp return.
    fn setjmp(buf: *mut JmpBuf) -> i32;
    /// Jumps back to the setjmp point. Never returns.
    fn longjmp(buf: *mut JmpBuf, val: i32) -> !;
}
```

The assembly saves/restores `ra`, `sp`, `s0`–`s11` (the RISC-V callee-saved registers). This is well-understood and small.

### Alloc error handler

```rust
#[alloc_error_handler]
fn on_alloc_error(layout: Layout) -> ! {
    if transaction_is_active() {
        // Signal OOM to the recovery point
        unsafe { longjmp(&mut RECOVERY_JMP_BUF, 1) }
    } else {
        panic!("OOM outside recoverable context: {:?}", layout);
    }
}
```

### Protected call site

```rust
/// Run `f(input)` with OOM recovery. On OOM, all allocations made by `f`
/// are freed and Err(OomError) is returned.
///
/// SAFETY: `f` must not mutate state outside its return value. On OOM,
/// `f` is aborted via longjmp — destructors do not run, and any external
/// mutations would leave the system inconsistent.
unsafe fn oom_protected<I, R>(input: I, f: fn(I) -> R) -> Result<R, OomError> {
    if setjmp(&mut RECOVERY_JMP_BUF) == 0 {
        // Normal path
        transaction_begin();
        let result = f(input);
        transaction_commit(); // stops tracking, leaves allocations in place
        Ok(result)
    } else {
        // Returned via longjmp — OOM occurred
        transaction_rollback(); // walks the linked list, frees everything
        Err(OomError)
    }
}
```

The `fn(I) -> R` signature (function pointer, not closure) ensures the callee cannot capture mutable references to external state. The only way data gets in is through `I` (moved), and the only way data gets out is through `R` (returned). This is enforced by the compiler.

The naming follows database transaction semantics: `begin` starts tracking, `commit` accepts the result (allocations persist), `rollback` reverts everything (all tracked allocations freed).

### Usage at the call site

```rust
// In ShaderRuntime::recompile()
let input = CompileInput { source, config, builtins };

// Drop old executable BEFORE compiling (existing pattern)
self.executable = None;

match unsafe { oom_protected(input, compile_shader) } {
    Ok(executable) => {
        self.executable = Some(executable);
        self.status = NodeStatus::Ok;
    }
    Err(OomError) => {
        self.status = NodeStatus::Error("OOM during shader compilation".into());
    }
}
```

---

## What Can Leak

Since `longjmp` skips destructors, anything with `Drop` that was live on the stack inside `f` will leak. The transactional allocator catches heap allocations, but other resources would not be cleaned up:

| Resource | Risk | Mitigation |
|----------|------|------------|
| Heap memory | Handled | Transaction rollback frees all tracked allocations |
| Cranelift IR, regalloc state | Handled | These are heap-allocated `Vec`s/`HashMap`s — caught by tracking |
| File handles | None | Compilation doesn't do I/O |
| Hardware peripherals | None | Compilation doesn't touch hardware |
| Mutex guards | Low | No mutexes in the compilation path (single-threaded) |
| `ManuallyDrop` / `Box::leak` | Low | Not used in compilation hot path |

The invariant — **compilation is a pure function from source to executable with no side effects** — should be documented and maintained.

---

## Crate Structure

```
lp-alloc/                         # new crate, #![no_std]
├── src/
│   ├── lib.rs                    # re-exports
│   ├── transaction.rs             # AllocTransaction, begin/commit/rollback
│   ├── tracked_allocator.rs      # OomRecoverableAllocator<A>
│   ├── jmpbuf.rs                 # JmpBuf type, oom_protected()
│   └── arch/
│       └── riscv32.S             # setjmp/longjmp assembly
```

Both `fw-emu` (via `lp-riscv-emu-guest`) and `fw-esp32` would use `lp-alloc` as their global allocator wrapper:

```rust
// fw-emu
#[global_allocator]
static ALLOC: OomRecoverableAllocator<LockedHeap> = ...;

// fw-esp32
#[global_allocator]
static ALLOC: OomRecoverableAllocator<EspHeap> = ...;
```

`lp-engine` calls `oom_protected()` without knowing which platform it's on.

---

## Linker Script Changes

### fw-emu (`lp-riscv-emu-guest/memory.ld`)

No changes needed for this approach. The `/DISCARD/ : { *(.eh_frame ...) }` stays — we don't need unwind tables.

### fw-esp32

No linker script changes needed.

---

## Testing Strategy

All testable in the emulator without hardware:

1. **Basic OOM recovery:** Shrink emulator heap (in linker script or at runtime). Load a project, trigger shader compilation. Verify OOM is caught, node enters `Error` state, server continues ticking.

2. **Memory reclamation:** Use `alloc-trace` to verify that after OOM recovery, heap usage returns to pre-compilation levels. No leaked allocations.

3. **Normal path correctness:** Run compilation within `oom_protected` with sufficient memory. Verify the allocator wrapper doesn't break normal behavior. Overhead should be limited to the 24 bytes/allocation during the transaction window.

4. **Stress test:** Compile, recover from OOM, compile again (with more memory). Verify the second compilation succeeds — the heap is clean after recovery, no fragmentation from leaked allocations.

5. **Double transaction prevention:** Verify that attempting to nest `oom_protected` calls panics (single static transaction).

---

## Future Layers (not in this design)

These build on top of the allocator recovery:

- **Project reload on OOM:** After a node enters OOM error state, unload and reload the project to reset all node state. Track reload attempts to avoid infinite loops.
- **ESP32 reboot on OOM:** If project reload also fails, write OOM state to flash (LittleFS) and trigger a software reset. On boot, check the flag and enter degraded mode if recovery already failed once.
- **Client notification:** New `ServerMessage` variant so `lp-cli` / future WiFi clients can display OOM status and recovery actions.
- **Proactive memory checks:** Before compilation, check `HEAP.free()` and skip if below a safety threshold.

---

## Open Questions

1. **Header overhead acceptability.** 24 bytes per allocation during compilation. Need to measure actual live allocation count during Cranelift compilation to confirm total overhead is manageable. The alloc-trace data should tell us this.

2. **`realloc` handling.** `GlobalAlloc::realloc` needs to update the tracking list (pointer changes). The wrapper must intercept `realloc`, unlink the old header, call inner realloc on the full (header + data) layout, and relink the new header.

3. **Alignment edge cases.** If the caller requests alignment > 4, the header may need additional padding to maintain alignment of the returned pointer. The header layout calculation must account for this.

4. **ESP32 `esp_alloc` compatibility.** Verify that wrapping `EspHeap` with the tracking allocator doesn't conflict with `esp_alloc`'s internal heap stats (`free()` / `used()`). The stats would be off by the header overhead during a transaction, which is acceptable.

5. **Input/output ownership for `oom_protected`.** The `input: I` parameter is moved into the function. On OOM, `I`'s destructor doesn't run (longjmp skips it). If `I` contains heap allocations (e.g., a `String`), those would leak — they aren't in the transaction because they were allocated before `transaction_begin()`. The input type should be borrows or `Copy` types to avoid this. A `CompileInput<'a>` with `&'a str` references is ideal.
