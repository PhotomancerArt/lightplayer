# RV32 Emulator: Host/Guest Memory Management Architecture

## Problem Statement

The emulator runs RISC-V guest code with its own memory space (`code` and `ram` vectors). Guest code (builtins, shaders) needs to access shared data structures like VMContext, textures, and uniform buffers. The question is: how does the host allocate and manage this memory so the guest can access it?

### The Current Bug (VMContext)

In `lpir-cranelift/src/emu_run.rs`:

```rust
// WRONG: Host stack pointer passed to guest
let header = VmContextHeader::default();
let vmctx_word = core::ptr::from_ref(&header) as usize as u32 as i32;
full.push(vmctx_word);  // Guest receives 0x7fff_xxxx host pointer, crashes
```

The builtin `__lp_vm_get_fuel_q32` tries to dereference this as a guest pointer, causing undefined behavior.

### The Fix

Allocate VMContext in **guest RAM**, pass the **guest address**:

```rust
// CORRECT: Guest RAM allocation via guest allocator
let vmctx_guest_addr = allocator.allocate(size_of::<VmContext>(), align_of::<VmContext>())?;
write_vmcontext_to_guest_ram(&mut load.ram, vmctx_guest_addr, &VmContext::default());
let vmctx_word = vmctx_guest_addr as i32;  // 0x8000_xxxx - valid in guest
```

## Architecture: Host-Managed Guest Allocations

### Why Not Bump Allocator?

Initial thought was a simple bump allocator in unused guest RAM. This fails for:
- Long-running emulator sessions (`fw-emu` from `lp-cli` for testing)
- Projects with many scenes/shaders requiring allocation/deallocation cycles
- Texture memory that needs to be freed when no longer used

### Solution: Host Calls Guest Allocator

The guest already has a `linked_list_allocator::LockedHeap`. We expose wrapper functions and call them from the host via the emulator's `call_function` API.

```
┌─────────────────┐     call_function      ┌──────────────────┐
│   Host (Rust)   │ ──────────────────────▶│  Guest (RV32)    │
│                 │                        │  linked_list     │
│ GuestAllocator  │ ◀────guest ptr────────│  _allocator      │
│                 │                        │                  │
└─────────────────┘                        └──────────────────┘
         │                                          │
         │ get_slice_mut(guest_ptr)                 │ load/store
         ▼                                          ▼
┌──────────────────────────────────────────────────────────┐
│              Guest RAM (load.ram: Vec<u8>)              │
│  [code][data][heap allocations (VMContext, textures)]     │
└──────────────────────────────────────────────────────────┘
```

## Implementation

### 1. Guest Exports Allocator Wrappers

In `lp-riscv-emu-guest` (linked into all guest binaries):

```rust
#[unsafe(no_mangle)]
pub extern "C" fn __lp_guest_alloc(size: usize, align: usize) -> *mut u8 {
    use core::alloc::{GlobalAlloc, Layout};
    let layout = Layout::from_size_align(size, align).ok()?;
    unsafe { HEAP_ALLOCATOR.alloc(layout) }
}

#[unsafe(no_mangle)]
pub extern "C" fn __lp_guest_free(ptr: *mut u8, size: usize, align: usize) {
    use core::alloc::{GlobalAlloc, Layout};
    if let Ok(layout) = Layout::from_size_align(size, align) {
        unsafe { HEAP_ALLOCATOR.dealloc(ptr, layout) }
    }
}
```

These appear in `ElfLoadInfo.symbol_map` after linking.

### 2. Host-Side `GuestAllocator`

```rust
/// Manages guest heap via synchronous calls into guest allocator.
pub struct GuestAllocator<'a> {
    load: &'a mut ElfLoadInfo,
    emu: &'a mut Riscv32Emulator,
    alloc_fn: u32,
    free_fn: u32,
}

impl<'a> GuestAllocator<'a> {
    pub fn new(load: &'a mut ElfLoadInfo, emu: &'a mut Riscv32Emulator) -> Result<Self, GuestAllocError> {
        // Look up __lp_guest_alloc and __lp_guest_free in symbol_map
    }

    /// Allocate in guest memory, return guest address (e.g., 0x8000_xxxx)
    pub fn allocate(&mut self, size: usize, align: usize) -> Result<u32, GuestAllocError> {
        // call_function(__lp_guest_alloc, args=[size, align])
        // Returns guest pointer
    }

    pub fn free(&mut self, ptr: u32, size: usize, align: usize) -> Result<(), GuestAllocError> {
        // call_function(__lp_guest_free, args=[ptr, size, align])
    }

    /// Access guest memory directly via host slice
    pub fn get_slice(&self, guest_ptr: u32, len: usize) -> &[u8] {
        let offset = (guest_ptr - DEFAULT_RAM_START) as usize;
        &self.load.ram[offset..offset + len]
    }

    pub fn get_slice_mut(&mut self, guest_ptr: u32, len: usize) -> &mut [u8] {
        let offset = (guest_ptr - DEFAULT_RAM_START) as usize;
        &mut self.load.ram[offset..offset + len]
    }
}
```

### 3. Usage Pattern: VMContext

```rust
pub fn glsl_q32_call_emulated(
    load: &mut ElfLoadInfo,
    ir: &IrModule,
    // ...
) -> Result<GlslReturn<GlslQ32>, CallError> {
    let mut emu = Riscv32Emulator::new(load.code.clone(), load.ram.clone());

    // Create allocator interface
    let mut allocator = GuestAllocator::new(load, &mut emu)?;

    // Allocate VMContext in guest memory
    let vmctx_size = size_of::<VmContext>();
    let vmctx_align = align_of::<VmContext>();
    let vmctx_addr = allocator.allocate(vmctx_size, vmctx_align)?;

    // Write initial VMContext data
    let vmctx = VmContext::default();
    let guest_slice = allocator.get_slice_mut(vmctx_addr, vmctx_size);
    guest_slice.copy_from_slice(vmctx_bytes(&vmctx));

    // Pass guest address to shader
    let args = vec![vmctx_addr as i32, /* other args */];
    let result = call_emulated_function(&mut emu, &args, ...);

    // VMContext persists for re-use, or free it:
    // allocator.free(vmctx_addr, vmctx_size, vmctx_align)?;

    result
}
```

### 4. Usage Pattern: Textures

Same pattern applies to texture data:

```rust
// Allocate texture in guest memory
let tex_size = width * height * 4;  // RGBA8
let tex_align = 4;
let tex_addr = allocator.allocate(tex_size, tex_align)?;

// Write pixel data
let tex_slice = allocator.get_slice_mut(tex_addr, tex_size);
tex_slice.copy_from_slice(&pixel_data);

// Pass to shader as sampler2D argument
let args = vec![vmctx_addr as i32, tex_addr as i32, width as i32, height as i32];
```

## Key Design Decisions

| Aspect | Decision | Rationale |
|--------|----------|-----------|
| **Allocator implementation** | Guest-side `linked_list_allocator` | Consistent with embedded target, supports free/realloc |
| **Host→guest calls** | Sync via `call_function` | Reuses existing emulator API, handles ABI/stack correctly |
| **Guest address space** | Raw `u32` (e.g., `0x8000_xxxx`) | Matches guest pointers, no translation needed |
| **Host access** | Direct slices into `ram: Vec<u8>` | Zero-copy, fast read/write |
| **Synchronization** | Implicit (emulator paused during host alloc) | No race conditions - guest not running during allocation |
| **Lifetime** | Explicit `allocate`/`free` or RAII wrapper | Caller controls when to release memory |

## Re-entrancy and Nested Calls

The emulator's `call_function` already saves/restores:
- Program counter (PC)
- General-purpose registers
- Stack pointer and return address

This means the host can safely call guest allocator functions even if the emulator was in the middle of executing guest code (e.g., from a syscall handler or trap). The nested call completes, the allocator returns a pointer, and the original guest execution resumes.

## Comparison to Prior Art

| System | Approach | Similarity to ours |
|--------|----------|------------------|
| **Wasmtime** | Guest memory is `&mut [u8]`, host accesses through bounds-checked slices | Same direct slice access pattern |
| **QEMU** | Guest physical memory mapped into host address space; guest ptr + offset = host ptr | We use offset into `Vec<u8>` instead of mmaps |
| **v8** | Pointer compression with heap base register | Not needed - 32-bit guest addresses fit in host registers |
| **WAMR** | Explicit `wasm_runtime_addr_app_to_native()` translation | We use fixed RAM_START offset instead |

Our approach is simpler because:
1. Guest is 32-bit, same endianness as host
2. Memory layout is fixed (code at 0x0, RAM at 0x80000000)
3. No virtual memory, no page tables, no ASID

## Open Questions

1. **OOM handling**: Guest allocator returns null on OOM. Host should propagate this as a clear error rather than crashing.

2. **Alignment requirements**: Textures may need 16-byte alignment for SIMD. Guest `linked_list_allocator` supports this via `Layout`.

3. **Fragmentation**: Long-running sessions may fragment the guest heap. We could add a "reset" that frees everything and reinitializes the heap, or implement a simple compaction if needed later.

4. **Multi-threading**: Currently single-threaded. If we add async shader compilation, need to ensure allocator calls are serialized or use thread-local heaps.

## Files to Modify

| File | Change |
|------|--------|
| `lp-riscv-emu-guest/src/allocator.rs` | Add `__lp_guest_alloc`, `__lp_guest_free` exports |
| `lpir-cranelift/src/guest_alloc.rs` | New module with `GuestAllocator` struct |
| `lpir-cranelift/src/emu_run.rs` | Use `GuestAllocator` for VMContext allocation |
| `lpir-cranelift/src/lib.rs` | Export `GuestAllocator` for downstream use |
