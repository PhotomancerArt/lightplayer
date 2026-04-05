# RV32 Emulator VMContext Support — Plan Notes

## Scope of Work

Enable the RV32 emulator to properly share memory structures (VMContext, textures, uniform buffers) between host and guest code. The current implementation passes host stack pointers to guest code, causing crashes when the guest tries to dereference them.

### Key Deliverables

1. **Guest-side allocator exports** (`__lp_guest_alloc`, `__lp_guest_free`) in `lp-riscv-emu-guest`
2. **Host-side `GuestAllocator`** in `lpir-cranelift` that calls guest allocator via emulator
3. **Fix `emu_run.rs`** to allocate VMContext in guest RAM instead of host stack
4. **Validation**: `vmcontext/fuel-read.glsl` test passes on `rv32.q32` target

## Current State

### The Bug

In `lp-glsl/lpir-cranelift/src/emu_run.rs` (lines 87-88 and 221-222):

```rust
// WRONG: Host stack pointer passed to guest
let header = VmContextHeader::default();
let vmctx_word = core::ptr::from_ref(&header) as usize as u32 as i32;
```

This creates a `VmContextHeader` on the host stack and passes its address to guest code. The guest receives a host pointer (e.g., `0x7fff_xxxx`) which is invalid in the guest address space, causing `InvalidMemoryAccess` when `__lp_get_fuel` tries to read it.

### Existing Infrastructure

1. **Guest allocator**: `lp-riscv-emu-guest/src/allocator.rs` has `LockedHeap` global allocator
2. **Emulator API**: `lp-riscv-emu` has `call_function()` for host→guest calls with state save/restore
3. **Symbol map**: `ElfLoadInfo.symbol_map` contains guest function addresses
4. **Memory layout**: Guest RAM starts at `0x80000000`, accessible via `load.ram: Vec<u8>`

## Questions

### Q1: Should `GuestAllocator` be a separate module or integrated into `emu_run.rs`?

**Context**: The `GuestAllocator` needs access to both `ElfLoadInfo` (for the RAM vector and symbol map) and `Riscv32Emulator` (for `call_function`).

**Suggested approach**: Create a new module `lpir-cranelift/src/guest_alloc.rs` with the `GuestAllocator` struct. This keeps the allocator logic separate from the run logic and allows reuse for other shared memory needs (textures, uniforms).

### Q2: How should the host locate `__lp_guest_alloc`/`__lp_guest_free` in the symbol map?

**Context**: The guest exports need to be findable by the host at runtime.

**Suggested approach**: The symbols will be in `ElfLoadInfo.symbol_map` after linking. `GuestAllocator::new()` will look them up by name and return an error if missing. This is consistent with how `glsl_q32_call_emulated` finds shader entry points.

### Q3: Should we initialize the guest heap explicitly or rely on the guest entry point?

**Context**: The guest allocator has `init_heap()` that must be called before any allocations.

**Suggested approach**: The guest entry point (`_start`) already calls `init_heap()` via the runtime initialization. When the host calls allocator functions, the heap is already initialized. We don't need explicit initialization on the host side.

### Q4: What error handling strategy for guest allocator OOM?

**Context**: The guest allocator returns `null` on out-of-memory.

**Suggested approach**: Define a `GuestAllocError` enum with variants for `OutOfMemory`, `SymbolNotFound`, and `EmulatorError`. Propagate these clearly to callers rather than panicking or returning invalid pointers.

## Notes

- The `vmcontext/fuel-read.glsl` test currently fails with `InvalidMemoryAccess { address: 1831541288, ... }` (address is a host stack pointer)
- After the fix, VMContext should be allocated in guest RAM at address `0x8000_xxxx`
- The same `GuestAllocator` pattern will be used for textures and uniform buffers in later milestones
