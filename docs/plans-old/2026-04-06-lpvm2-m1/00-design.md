# LPVM Trait Redesign — Design

## Scope

This plan implements Milestone 1 of the LPVM2 roadmap: adding shared memory
support to the LPVM trait definitions. The core changes are:

1. Add `LpvmMemory` trait with `alloc()`, `free()`, `realloc()` methods
2. Define `ShaderPtr` struct with `native_ptr()` and `guest_value()` methods
3. Define `AllocError` enum for allocation failures
4. Update `LpvmEngine` to expose `memory() -> &dyn LpvmMemory`
5. Update documentation to clarify `VmContext` stays per-instance
6. Unit tests for the new types and trait coherence

## File Structure

```
lp-shader/lpvm/
├── Cargo.toml
└── src/
    ├── lib.rs                  # UPDATE: re-export ShaderPtr, LpvmMemory, AllocError
    ├── engine.rs               # UPDATE: add LpvmEngine::memory(&self) method
    ├── module.rs               # (no changes - memory injected at compile time)
    ├── instance.rs             # (no changes)
    ├── vmcontext.rs            # UPDATE: doc comment clarifying per-instance scope
    ├── data.rs                 # (no changes)
    ├── shader_ptr.rs           # NEW: ShaderPtr struct with native/guest pointers
    ├── memory.rs               # NEW: LpvmMemory trait + AllocError enum
    └── tests/
        └── shader_ptr_tests.rs # NEW: unit tests for ShaderPtr construction
```

## Conceptual Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                         LpvmEngine                          │
│  ┌──────────────────────┐        ┌──────────────────────┐  │
│  │  compile()          │        │  memory()            │  │
│  │  -> Module           │        │  -> &dyn LpvmMemory   │  │
│  └──────────────────────┘        └──────────────────────┘  │
│                                                              │
│  (engine owns shared memory, injects into modules at         │
│   compile time, instances use at instantiate time)             │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
                    ┌─────────────────┐
                    │   LpvmMemory    │  (object-safe trait)
                    │  ─────────────  │
                    │  alloc(size)    │  ──► ShaderPtr
                    │  free(ptr)      │      /         \
                    │  realloc(ptr)   │     /           \
                    └─────────────────┘    /             \
                                          /               \
                              ┌──────────┐                 \
                              │ native   │                  \
                              │ (*mut u8)│                   \
                              │          │                    \
                              │ host     │                     \
                              │ writes   │                      \
                              │ texture  │                       \
                              └──────────┘                        \
                                                                   \
                              ┌──────────────────┐                \
                              │ guest (u64)       │                 \
                              │ ────────────────  │                  \
                              │ 32-bit: lower 32  │                   \
                              │ 64-bit: full u64  │                    \
                              │                   │                     \
                              │ shader uniform    │                      \
                              │ (Load/Store)      │                       \
                              └──────────────────┘                        \
                                                                         \
                              ┌──────────────────────┐                    \
                              │   VmContext          │                     \
                              │  (per-instance)      │                      \
                              │  fuel, trap_handler  │                       \
                              └──────────────────────┘
```

## Main Components

### ShaderPtr

A concrete struct pairing native and guest pointer values. Backends construct
it with appropriate values for their memory model.

```rust
pub struct ShaderPtr {
    native: *mut u8,   // Host can read/write via this pointer (unsafe)
    guest: u64,        // Value passed to shader in uniforms
}
```

- **JIT (32-bit)**: `native` = host pointer, `guest` = same value as u64
- **JIT (64-bit)**: `native` = host pointer, `guest` = same value as u64
- **WASM**: `native` = `memory_base + offset`, `guest` = offset as u64
- **Emulator**: `native` = `ram_base + (addr - shared_start)`, `guest` = address as u64

### LpvmMemory Trait

Object-safe trait for shared memory allocation. Engine implementations
provide this via interior mutability (atomics, RefCell, Mutex, etc.).

```rust
pub trait LpvmMemory {
    fn alloc(&self, size: usize) -> Result<ShaderPtr, AllocError>;
    fn free(&self, ptr: ShaderPtr);
    fn realloc(&self, ptr: ShaderPtr, new_size: usize) -> Result<ShaderPtr, AllocError>;
}
```

Returns concrete `AllocError` (not associated type) for object safety.

### AllocError

Backend-independent allocation failure reasons:

```rust
pub enum AllocError {
    OutOfMemory,      // Cannot satisfy allocation request
    InvalidSize,      // Size 0 or overflows
    InvalidPointer,   // free/realloc of invalid pointer
}
```

### Updated LpvmEngine

Adds `memory()` method returning `&dyn LpvmMemory`. External code allocates
via `engine.memory().alloc(...)`.

```rust
pub trait LpvmEngine {
    type Module: LpvmModule;
    type Error: Display;

    fn compile(&self, ir: &IrModule, meta: &LpsModuleSig) -> Result<Self::Module, Self::Error>;
    fn memory(&self) -> &dyn LpvmMemory;
}
```

Modules receive memory reference at `compile()` time (engine injects).
Instances use it at `instantiate()` time. The `instantiate()` signature
is unchanged — no new parameters needed.

### VmContext Documentation

Updated doc comment clarifying scope:

```rust
/// Per-instance VM context (fuel, trap handler, metadata pointer).
///
/// This struct is **per-instance**, not shared across instances. For shared
/// data (textures, globals), use uniforms containing `ShaderPtr` guest values.
/// `ShaderPtr::native_ptr()` gives the host direct access to shared memory.
```

## Safety

- `ShaderPtr::native_ptr()` returns `*mut u8` — using it is `unsafe`
- Shared memory is inherently shared-mutable; host access must be synchronized
- Backends ensure their implementations are thread-safe
- No `Send`/`Sync` bounds on `ShaderPtr` — document the contract instead

## Testing

Unit tests cover:
- `ShaderPtr` construction and field access
- `AllocError` variants and Display
- Trait coherence (dummy impls compile)
- Documentation examples