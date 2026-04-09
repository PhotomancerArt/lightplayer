# Notes — LPVM2: Shared Memory and Backend Completion

## Scope

This roadmap picks up from LPVM (2026-04-04) after M1–M3 completion. It
addresses a design gap discovered during M4 planning: the LPVM traits have no
memory concept, which prevents shared memory across instances (required for
textures, cross-shader data).

The scope covers:

1. Redesigning LPVM traits to include shared memory
2. Fixing the WASM backends (both create per-instance memory today)
3. Building a new rv32 emulator with proper architecture
4. Creating LPVM trait implementations for the new emulator
5. Migrating filetests to LPVM
6. Migrating lp-engine to LPVM (backend-agnostic)
7. Cleanup

## Current State

### What's done (from LPVM roadmap)

- **M1**: Renames, moves, new types. `lps-shared`, `lpvm` core crate with
  traits, `LpvmData`, `VmContext`. Done.
- **M2**: `lpvm-wasm`. WASM backend with wasmtime and browser runtimes,
  `LpvmEngine`/`LpvmModule`/`LpvmInstance` implementations. Done.
- **M3**: `lpvm-cranelift` JIT traits. `CraneliftEngine`/`CraneliftModule`/
  `CraneliftInstance` alongside the existing `JitModule` API. Done.

### What's wrong

**The LPVM traits have no memory concept.** The current trait signatures:

```rust
trait LpvmEngine {
    type Module: LpvmModule;
    fn compile(&self, ir: &IrModule, meta: &LpsModuleSig) -> Result<Self::Module, ...>;
}

trait LpvmModule {
    type Instance: LpvmInstance;
    fn instantiate(&self) -> Result<Self::Instance, ...>;
}

trait LpvmInstance {
    fn call(&mut self, name: &str, args: &[LpsValue]) -> Result<LpsValue, ...>;
}
```

`instantiate()` takes no arguments — there's no way to pass shared memory.
Each backend creates its own memory internally:

- **WASM (wasmtime)**: `Memory::new()` inside `instantiate_wasm_module()`
- **WASM (browser)**: `WebAssembly.Memory::new()` inside `instantiate_shader()`
- **Cranelift JIT**: `vmctx_buf: Vec<u8>` allocated in `CraneliftInstance::new()`

All three backends create **per-instance memory**. There is no way to share
memory across instances (needed for textures, shared globals).

**The rv32 emulator combines code, state, and memory.** `Riscv32Emulator`
owns registers, PC, `Memory` (code + ram), traps, serial, timing — all in one
struct. This doesn't map to the Module/Instance/Memory separation LPVM needs.
The emulator is also aging and could benefit from a fresh architecture.

### What works

- `lp-engine` uses `lpvm_cranelift::jit()` + `DirectCall` for the production
  render path. This works and is fast.
- Filetests use `GlslExecutable` trait (via `LpirRv32Executable`,
  `LpirJitExecutable`, WASM executables). This works.
- `fw-tests` use `Riscv32Emulator` directly for ELF-level testing.
- The LPVM trait implementations (M2, M3) work for single-instance usage.

### Backends and their consumers

| Backend | Crate | Used by |
|---------|-------|---------|
| Cranelift JIT | `lpvm-cranelift` | `lp-engine` (via `jit()` + `DirectCall`) |
| Cranelift JIT (LPVM) | `lpvm-cranelift` | nothing yet (M3 just added) |
| RV32 emulator | `lpvm-cranelift` (emu_run) | `lps-filetests` (via `LpirRv32Executable`) |
| WASM wasmtime | `lpvm-wasm` | `lps-filetests` (via `WasmtimeLpvmExecutable`) |
| WASM browser | `lpvm-wasm` | `web-demo` |
| LPIR interpreter | `lpir::interp` | `lps-filetests` |

### Key files

- Trait definitions: `lp-shader/lpvm/src/{engine,module,instance}.rs`
- VMContext: `lp-shader/lpvm/src/vmcontext.rs`
- WASM linking (wasmtime): `lp-shader/lpvm-wasm/src/rt_wasmtime/link.rs`
- WASM linking (browser): `lp-shader/lpvm-wasm/src/rt_browser/link.rs`
- Cranelift LPVM: `lp-shader/lpvm-cranelift/src/lpvm_{engine,module,instance}.rs`
- Emulator state: `lp-riscv/lp-riscv-emu/src/emu/emulator/state.rs`
- Emulator memory: `lp-riscv/lp-riscv-emu/src/emu/memory.rs`
- Executor dispatch: `lp-riscv/lp-riscv-emu/src/emu/executor/mod.rs`
- Engine shader runtime: `lp-core/lp-engine/src/nodes/shader/runtime.rs`

## Questions

### Q1: Memory ownership model

Where does shared memory live in the trait hierarchy?

**Context**: Three backends need different memory models:
- **Cranelift JIT**: Host memory. Sharing is free — just pass the same pointer.
  VMContext already has a metadata pointer field. Shared data would be another
  pointer in VMContext (or VMContext itself points to a shared allocation).
- **WASM**: `WebAssembly.Memory` (browser) or `wasmtime::Memory` (native).
  Created with a size spec. Multiple instances import the same Memory object.
- **RV32 emu**: RAM buffer. Code is separate. Multiple instances could share
  the same RAM allocation.

**Options**:

(A) **Engine-owned memory**: Engine creates and owns memory. Passed to
    `instantiate()`:
    ```
    engine.create_memory(spec) -> Memory
    module.instantiate(&memory) -> Instance
    ```

(B) **Module-scoped memory**: Module creates memory internally. All instances
    from the same module share it.
    ```
    module.instantiate() -> Instance  // memory is internal to module
    ```

(C) **External memory**: Memory is a standalone type, created independently,
    passed wherever needed:
    ```
    let memory = LpvmMemory::new(spec);
    module.instantiate(&mut memory) -> Instance
    ```

**Suggested answer**: Option (A), engine-owned. The engine knows the backend
constraints (page sizes for WASM, alignment for JIT). For Cranelift, creating
"memory" is trivially allocating a buffer. For WASM, it's creating
`WebAssembly.Memory` with the right page count. The engine is the natural
owner because it outlives both modules and instances.

**Answer**: Option (A), engine-owned. Cross-module memory sharing (e.g.,
shader A reads a texture written by shader B) requires memory to live at the
engine level. The engine outlives modules and instances, knows backend
constraints, and maps cleanly to WASM's model.

### Q2: VMContext vs shared memory

How do per-instance state (fuel, trap handler) and shared data (textures,
globals) relate?

**Context**: Today `VmContext` is a flat `repr(C)` struct:
```rust
pub struct VmContext {
    pub fuel: u64,
    pub trap_handler: u32,
    pub metadata: *const LpsType,
}
```

Fuel is per-instance. Textures/globals would be shared across instances.

**Options**:

(A) **VMContext contains a pointer to shared memory**: Per-instance VMContext
    header + pointer to shared allocation. Instance owns VMContext, shared
    memory is external.

(B) **VMContext IS shared memory**: One big allocation. Fuel/trap fields are
    at known offsets. Instances index into it. (Conflicts with per-instance
    fuel.)

(C) **Two separate concepts**: VMContext stays per-instance (fuel, etc.).
    Shared memory is a separate allocation accessed via the engine/memory
    handle. Functions receive both via arguments or indirection.

**Suggested answer**: (superseded — see answer)

**Answer**: Neither A, B, nor C as originally framed. The shared memory model
is an **allocator/heap** owned by the engine, not a single buffer pointed to
from VMContext. No VMContext changes needed for this.

The engine provides `alloc(size) -> ShaderPtr`, `realloc`, `free`. A
`ShaderPtr` has two faces:
- **native pointer**: host Rust can read/write the data directly
- **guest value**: what the shader sees (offset for WASM, native pointer for
  JIT, RAM address for emu)

Textures are allocated via the engine, producing a `ShaderPtr`. The
`ShaderPtr` is passed into the shader as a uniform. The compiler generates
`Load`/`Store` from the uniform value + pixel offset — direct memory access,
no special indirection.

Per-backend:
- **Cranelift JIT**: native == guest (same address space). `alloc` is host
  allocation. `ShaderPtr` wraps a single pointer.
- **WASM**: guest is an offset into linear memory. native is
  `linear_memory_base + offset`. `alloc` is a bump/free-list within linear
  memory.
- **RV32 emu**: guest is a RAM address. native is
  `host_ram_base + (guest_addr - ram_start)`. `alloc` is a bump/free-list
  within the emulator's RAM.

VMContext stays per-instance (fuel, trap handler). Shared data access is
through uniforms holding `ShaderPtr` guest values, not through VMContext
pointers.

### Q3: Where does the new emulator live?

**Context**: The current emulator is `lp-riscv/lp-riscv-emu`. It has ~8k
lines of instruction execution code in `executor/` that is correct and
well-tested. The state management (`Memory`, `Riscv32Emulator`) is what
needs redesigning. The user wants to build the new emulator alongside the old
one, reusing instruction execution logic, then remove the old one.

**Options**:

(A) **New module in existing crate**: `lp-riscv-emu/src/emu_v2/` alongside
    `lp-riscv-emu/src/emu/`. Shares `executor/` directly. Old API stays.
    Delete `emu/` when migration is complete.

(B) **New crate**: `lp-riscv/lp-riscv-emu-v2` (or better name). References
    `lp-riscv-emu`'s executor via dependency or extracts executor into a
    shared sub-crate.

(C) **Extract executor, rebuild around it**: Pull `executor/` into its own
    crate (or module), then build the new emulator as a new consumer of the
    executor alongside the old one.

**Suggested answer**: Option (A).

**Answer**: Option (A). New module in the existing crate (`emu_v2/` or
similar). Shares `executor/` directly. Old `emu/` stays until consumers
migrate, then gets deleted and the new module can be renamed.

### Q4: Where does the LPVM rv32 implementation live?

**Context**: The old plan (LPVM M4) put it in `lpvm-cranelift` behind a
`riscv32-emu` feature flag, since Cranelift generates the RV32 object code
that the emulator runs. But with a new emulator, the boundaries are cleaner.

**Options**:

(A) **In `lpvm-cranelift`** (feature flag): Same as old plan. `CraneliftEngine`
    gains a `compile_rv32()` or the engine detects the target. One crate for
    both JIT and emulated execution.

(B) **Separate `lpvm-rv32emu` crate**: Depends on `lpvm-cranelift` (for RV32
    object codegen) and `lp-riscv-emu` (for the new emulator). Cleaner
    separation.

**Suggested answer**: (superseded — see answer)

**Answer**: Option (B). Separate `lpvm-emu` crate, depends on
`lpvm-cranelift` (for RV32 object codegen) and `lp-riscv-emu` (for the
new emulator). Most consumers of `lpvm-cranelift` don't want the emulator
dependency, so keeping it separate is cleaner.

**Future work note**: Ideal long-term crate split would be three crates:
`lpir-cranelift` (codegen only), `lpvm-jit` (JIT execution), `lpvm-emu`
(emulated execution). Not worth doing now — `lpvm-cranelift` handles both
codegen and JIT, `lpvm-emu` wraps the emulator path.

### Q5: Hot path and DirectCall

How does the render hot path work when lp-engine becomes generic over LPVM?

**Context**: Today `lp-engine` uses `DirectCall::call_i32_buf()` directly.
This is a raw function pointer call with no marshaling overhead. The LPVM
trait's `call()` method does LpsValue marshaling, which is too slow for
per-pixel rendering.

**Options**:

(A) **Add a fast-path method to LpvmModule**: e.g., `direct_call() -> Option<DirectCall>`.
    Backends that support it return Some, others None. Engine uses it when
    available, falls back to trait `call()`.

(B) **Add a raw call method to LpvmInstance**: e.g., `call_raw(&mut self, name, args: &[i32], ret: &mut [i32])`. Lower-level than `call()` but still through the trait.

(C) **Keep DirectCall as a backend-specific optimization**: Engine is generic
    but firmware crates can downcast or use backend-specific APIs for the hot
    path. The trait interface is for non-hot-path usage (filetests, setup).

(D) **Defer to M6**: Design the fast path when we actually migrate the engine.
    The trait works for filetests (M5). Engine migration (M6) can add what it
    needs.

**Suggested answer**: (superseded — see answer)

**Answer**: Option (C) for now. Backend-specific APIs for the hot path.
Engine uses concrete backend types (downcast or cfg-gated) for rendering.
The trait interface is for non-hot-path usage (filetests, setup, etc.).

**Future work note**: Medium-term goal is to abstract texture rendering at
the LPVM level itself, with backend-specific implementations. Since it's the
hot path, impl-specific code matters. Could even compile a `renderTexture`
function into the binary so the render loop doesn't cross the shader
boundary at all. Too early to design now — needs more experience with the
texture/uniform system first.

### Q6: Scope of the WASM memory fix

What exactly needs to change in the WASM backends?

**Context**: Both wasmtime and browser paths create `Memory::new()` inside
the instantiation flow. The fix is: memory should be created externally and
passed in.

**Specific changes needed**:
- `instantiate_wasm_module()` should accept an existing `Memory` instead of
  creating one
- `instantiate_shader()` (browser) should accept an existing
  `WebAssembly.Memory`
- `WasmLpvmModule::instantiate()` needs to take a memory parameter (trait change)
- The memory type spec (page count, etc.) needs to come from somewhere —
  probably the compiled module knows what it needs

**Suggested answer**: The WASM fix is straightforward once the trait design
is settled. The compiled module knows its memory requirements (page count from
`env_memory` / `EnvMemorySpec`). The engine creates memory using those
requirements. `instantiate()` receives the memory handle. Both wasmtime and
browser paths need the same structural change.

**Answer**: Scope is correct. Engine creates linear memory, instances import
it, engine provides alloc/free within the memory. Start small, grow as
needed. Both wasmtime and browser backends get the same structural change.

**Important browser caveat**: For `fw-wasm` (firmware running as WASM in
browser), shader instances must import the **firmware's own
`WebAssembly.Memory`**, not create separate memory. This is required for
`ShaderPtr.native_ptr()` to work — the firmware needs direct pointer access
to shader data (textures). Multiple WASM modules sharing one Memory is a
supported pattern.

Security implication: a shader exploit could access all of fw-wasm's memory.
This is acceptable because the architecture already isolates the firmware
from the main app (lp-studio) via client/server message passing. The firmware
is a sandbox.

Implementation concerns for fw-wasm (future, not this roadmap):
- Shadow stacks need to be partitioned within shared memory
- Shader stack/heap regions within shared memory
- Builtins that access memory operate on the shared space
- Each shader instance needs its own stack/shadow-stack region

These are `fw-wasm` milestone concerns, not trait design concerns. The trait
design (engine owns memory, instantiate receives it, alloc returns ShaderPtr)
accommodates this model.

### Q7: Does the emulator need full shared memory support?

**Context**: The emulator is only used for filetests (file-scoped, no
cross-shader anything) and fw-tests (ELF-level testing). It's not used in
production by lp-engine. Cross-shader memory sharing (textures) is a
production requirement that only matters for Cranelift JIT and WASM.

However, a uniform architecture across backends makes the system cleaner and
ensures filetests can exercise the shared memory API even if they don't use
multiple shaders simultaneously.

**Options**:

(A) **Full shared memory**: New emulator separates code, RAM, and state.
    LPVM wrapper supports shared memory across instances. Filetests can
    optionally test shared memory scenarios.

(B) **Structural separation only**: New emulator separates code and state
    (for Module/Instance), but RAM is per-instance. Simpler, sufficient for
    filetests. Shared memory tested via WASM and Cranelift backends only.

**Suggested answer**: (superseded — see answer)

**Answer**: Simpler approach than either A or B. Instead of forking the
emulator or building a new one, add a third memory region ("shared memory")
to the existing `Memory` struct in `lp-riscv-emu`:

```
Address space:
  0x00000000 - code (read-only)
  0x40000000 - shared memory (read-write, engine-owned)
  0x80000000 - RAM (read-write, per-instance)
```

One extra `if` branch per memory access. Negligible for a testing emulator.

LPVM wrapper (`lpvm-emu` crate):
- `EmuEngine`: owns the shared memory Vec, provides alloc/free within it
- `EmuModule`: holds compiled RV32 object code, symbol map, traps (moved
  from `lpvm-cranelift/src/emu_run.rs`)
- `EmuInstance`: creates a `Riscv32Emulator` with the module's code, its
  own RAM, and a reference to the engine's shared memory

This eliminates the "fork the emulator" milestone entirely. The emulator
stays monolithic (code + RAM + registers in one struct), which is fine for
filetests. The only change to `lp-riscv-emu` is the third memory region.

`emu_run.rs` moves from `lpvm-cranelift` to `lpvm-emu` to remove the
emulator dependency from `lpvm-cranelift`.

**Future work note**: The ideal long-term crate split (`lpir-cranelift` /
`lpvm-jit` / `lpvm-emu`) noted in Q4 would clean this up further.

## Notes

(Accumulated during question iteration.)

- The emulator fork/rewrite was de-scoped in favor of adding a shared memory
  region to the existing emulator. Much simpler, same result for filetests.
- `emu_run.rs` should move from `lpvm-cranelift` to `lpvm-emu` to remove the
  `lp-riscv-emu` dependency from `lpvm-cranelift`.
- The current emulator has NO memory mapping: just two flat `Vec<u8>` (code
  at 0x0, RAM at 0x80000000) with a simple address comparison for dispatch.
  Adding shared memory at 0x40000000 makes it three-way dispatch.
- For fw-wasm (browser), shader instances must share the firmware's own
  `WebAssembly.Memory`. This gives the firmware direct pointer access to
  shader data. Security is acceptable due to client/server isolation between
  firmware and the main app.
- fw-wasm shared memory concerns (shadow stack partitioning, shader
  stack/heap regions, builtins) are deferred to the fw-wasm milestone.
- Medium-term goal: abstract texture rendering at the LPVM level with
  backend-specific implementations. Could compile a `renderTexture` function
  into the binary so the render loop doesn't cross the shader boundary.
- Long-term ideal crate split: `lpir-cranelift` (codegen), `lpvm-jit`
  (JIT execution), `lpvm-emu` (emulated execution). Not worth doing now.
