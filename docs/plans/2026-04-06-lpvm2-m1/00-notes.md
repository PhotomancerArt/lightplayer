# LPVM Trait Redesign — Notes

## Scope

This plan implements Milestone 1 of the LPVM2 roadmap: adding shared memory
support to the LPVM trait definitions. The changes are:

1. Add `alloc()`, `realloc()`, `free()` to `LpvmEngine`
2. Define `ShaderPtr` type with `native_ptr()` and `guest_value()` methods
3. Update `LpvmModule::instantiate()` to accept an engine reference
4. Update `VmContext` documentation to clarify it stays per-instance
5. Tests for the new types and trait coherence

## Current State

### Trait definitions

`lp-shader/lpvm/src/{engine,module,instance}.rs` define three traits:

```rust
// engine.rs
trait LpvmEngine {
    type Module: LpvmModule;
    type Error: Display;
    fn compile(&self, ir: &IrModule, meta: &LpsModuleSig) -> Result<Self::Module, Self::Error>;
}

// module.rs
trait LpvmModule {
    type Instance: LpvmInstance;
    type Error: Display;
    fn signatures(&self) -> &LpsModuleSig;
    fn instantiate(&self) -> Result<Self::Instance, Self::Error>;  // ← takes no args
}

// instance.rs
trait LpvmInstance {
    type Error: Display;
    fn call(&mut self, name: &str, args: &[LpsValue]) -> Result<LpsValue, Self::Error>;
}
```

### VMContext

`VmContext` is defined in `lp-shader/lpvm/src/vmcontext.rs` as:

```rust
#[repr(C)]
pub struct VmContext {
    pub fuel: u64,
    pub trap_handler: u32,
    pub metadata: *const LpsType,
}
```

It is per-instance (fuel, trap handler). No shared memory pointer.

### LpvmData

`LpvmData` (in `data.rs`) is a host-side type for shader data layout. Not
directly related to shared memory — it manages byte buffers for value
marshaling.

### Existing backend implementations

- `lpvm-wasm` — implements the traits but creates per-instance memory
- `lpvm-cranelift` — implements the traits but allocates per-instance VMContext

Both will be updated in later milestones to use the new signatures.

## Questions

### Q1: Should `instantiate()` take `&self` or `&mut self` on the engine?

**Context**: `LpvmModule::instantiate()` needs to access the engine's shared
memory allocator. The trait signature is:

```rust
fn instantiate(&self, engine: &E) -> Result<Self::Instance, Self::Error>;
```

where `E: LpvmEngine`. But the engine's `alloc()` needs `&mut self` for the
bump allocator / free-list.

**Options**:

(A) `instantiate(&self, engine: &mut E)` — mutable reference to engine.

(B) `instantiate(&self, engine: &E)` with interior mutability — engine uses
`RefCell` or `Mutex` internally for the allocator.

(C) `instantiate(&self)` with engine reference stored in module at compile
time — module holds `Arc<Engine>` or similar.

**Suggested answer**: (superseded — see answer)

**Answer**: None of the above. `instantiate()` keeps its current signature
(`fn instantiate(&self) -> Result<...>`). Memory is not passed as an argument.

Memory is engine-scoped and exposed via `&dyn LpvmMemory`:

```rust
trait LpvmEngine {
    fn memory(&self) -> &dyn LpvmMemory;
    fn compile(&self, ...) -> Result<Self::Module, Self::Error>;
}
```

The engine injects its memory reference into modules at `compile()` time.
Modules wire it into instances at `instantiate()` time. The trait surface
for module/instance is unchanged.

`LpvmMemory` uses `&self` with interior mutability for `alloc()`. It uses
a concrete `AllocError` type (not an associated type) so it's object-safe
and works as `dyn LpvmMemory`.

External code that needs to allocate calls `engine.memory().alloc(...)`.
Memory lifetime is tied to the engine, which is correct — you don't want
memory outliving the engine.

### Q2: Should `ShaderPtr` be a trait or a concrete type?

**Context**: `ShaderPtr` needs to provide:
- `native_ptr()` -> `*mut u8` for host direct access
- `guest_value()` -> `i32` for what the shader sees (offset, address, etc.)

Different backends have different representations:
- JIT: both are the same pointer
- WASM: native is `memory_base + offset`, guest is offset
- Emulator: native is `ram_base + (addr - shared_start)`, guest is address

**Options**:

(A) **Concrete struct with backend-specific inner type**:
```rust
pub struct ShaderPtr {
    native: *mut u8,
    guest: i32,
}
```
Engine constructs it with the right values for its backend.

(B) **Trait with associated types**:
```rust
trait ShaderPtr {
    fn native_ptr(&self) -> *mut u8;
    fn guest_value(&self) -> i32;
}
```
Each backend defines its own `ShaderPtr` type.

**Suggested answer**: (A) Concrete struct. The two values are always paired.
Backends compute them differently at construction time, but the struct itself
is uniform. Simpler for consumers, no generic parameters on `LpvmEngine` for
the pointer type.

**Answer**: (A) Concrete `ShaderPtr` struct with `native: *mut u8` and
`guest: i32`. Backends construct it with the appropriate values for their
memory model.

### Q3: Error handling for `alloc()` failure

**Context**: `LpvmEngine` has an associated `type Error`. Should
`alloc()` return `Self::Error` or a separate error type?

**Options**:

(A) `fn alloc(&mut self, size: usize) -> Result<ShaderPtr, Self::Error>` —
uses the same error type as `compile()`.

(B) `fn alloc(&mut self, size: usize) -> Result<ShaderPtr, AllocError>` —
separate error type (OOM, invalid size, etc.).

**Suggested answer**: (superseded — see answer)

**Answer**: (A) Concrete `AllocError`. Since `LpvmMemory` needs to be
object-safe for `dyn` usage, it can't have associated types. A small
concrete enum: `OutOfMemory`, `InvalidSize`, `InvalidPointer` (for
`realloc`/`free`).

### Q4: Should `realloc()` be required?

**Context**: The trait has `alloc()` and `free()`. `realloc()` is useful
for resizing textures but not strictly necessary (can alloc/copy/free).

**Options**:

(A) Include `realloc()` in the trait — provides efficient resizing when
backend supports it.

(B) Only `alloc()`/`free()` — `realloc()` is a utility function in terms
of those, or callers manage resizing manually.

**Suggested answer**: (A) Include it. Texture resizing is a common
operation. WASM `memory.grow()` is the backend primitive; exposing it
through `realloc()` keeps the trait useful.

**Answer**: (A) Include `realloc()` in `LpvmMemory`.

### Q5: Thread safety bounds on `ShaderPtr`

**Context**: `ShaderPtr` has a native pointer. It may be accessed from
different threads:
- Host writes texture data while shader reads it
- Multiple shader instances read the same texture

**Options**:

(A) `ShaderPtr: Send + Sync` — always thread-safe. Backends use atomics
or ensure the memory is thread-safe.

(B) `ShaderPtr: Send` only — host can pass between threads, but
synchronization is caller's responsibility.

(C) No bounds — backend decides. `LpvmEngine` documentation specifies
the contract.

**Suggested answer**: (C) No bounds, document the contract. The shared
memory is inherently shared-mutable. Backends must ensure their
implementation is safe, but the trait doesn't need to encode it. JIT and
WASM linear memory are thread-safe (properly synchronized). The emulator
for filetests is single-threaded.

**Answer**: (C) No bounds on `ShaderPtr`. Document the safety contract:
the memory is shared-mutable, host access must be synchronized.

## Notes

(Accumulated during question iteration.)

- `ShaderPtr::guest_value()` returns `u64` as a universal carrier. 32-bit targets
  (WASM, RV32, ESP32-C6) use the lower 32 bits. 64-bit JIT uses the full width.
  The compiler backend knows the target pointer width when generating code and
  emits appropriate pointer-width operations. Zero-extension for 32-bit is
  acceptable overhead (texture handles are low-volume).
