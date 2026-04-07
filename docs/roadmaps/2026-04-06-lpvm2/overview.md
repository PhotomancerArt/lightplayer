# LPVM2 Roadmap — Shared Memory and Backend Completion

## Motivation / Rationale

LPVM (the runtime abstraction for executing compiled shaders) was built in
three milestones (M1–M3 of the original LPVM roadmap). The trait design
works for single-instance usage, but during M4 planning a fundamental gap
was discovered: **the LPVM traits have no memory concept**.

`LpvmModule::instantiate()` takes no arguments. Each backend creates its own
memory internally — WASM creates a fresh `Memory::new()` per instance,
Cranelift allocates a per-instance VMContext buffer. There is no way to share
memory across instances.

This matters because:

- **Textures** are shared data. Shader A writes to a texture, shader B reads
  it. Both need access to the same memory region.
- **Cross-shader globals** (future) need shared storage.
- **fw-wasm** (browser firmware target) needs shader instances to import the
  firmware's own `WebAssembly.Memory` for direct pointer access.

The fix is architectural: add an allocator/heap to `LpvmEngine`, introduce
`ShaderPtr` (dual native/guest pointer), and update all backends. Then migrate
consumers (filetests, engine) to the new trait interface.

### Current pain points

1. WASM backends create per-instance memory — no sharing possible.
2. Cranelift JIT traits (M3) have no memory parameter — `instantiate()` is
   self-contained.
3. The RV32 emulator has no LPVM implementation at all — filetests still use
   the old `GlslExecutable` trait via `LpirRv32Executable`.
4. `lp-engine` directly uses `lpvm_cranelift::jit()` + `DirectCall` — not
   backend-agnostic, can't swap in WASM for fw-wasm.

## Architecture / Design

### Memory model

The engine owns a shared memory region and provides an allocator interface:

```rust
trait LpvmEngine {
    type Module: LpvmModule;
    type Error: Display;

    fn compile(&self, ir: &IrModule, meta: &LpsModuleSig) -> Result<Self::Module, Self::Error>;
    fn alloc(&mut self, size: usize) -> Result<ShaderPtr, Self::Error>;
    fn realloc(&mut self, ptr: ShaderPtr, new_size: usize) -> Result<ShaderPtr, Self::Error>;
    fn free(&mut self, ptr: ShaderPtr);
}

trait LpvmModule {
    type Instance: LpvmInstance;
    type Error: Display;

    fn signatures(&self) -> &LpsModuleSig;
    fn instantiate(&self, engine: &Self::Engine) -> Result<Self::Instance, Self::Error>;
}
```

`ShaderPtr` has two faces:
- **`native_ptr()`**: host pointer for Rust code to read/write data directly
- **`guest_value()`**: what the shader sees (offset for WASM, native pointer
  for JIT, RAM address for emulator)

Textures are allocated via the engine, producing a `ShaderPtr`. The pointer
is passed into shaders as a uniform. The compiler generates `Load`/`Store`
from the uniform value + offset — direct memory access, no indirection.

### Per-backend memory implementation

| Backend | `alloc()` | `native_ptr()` | `guest_value()` |
|---------|-----------|-----------------|------------------|
| Cranelift JIT | Host allocator | Same pointer | Same pointer (cast) |
| WASM (wasmtime) | Bump/free-list in linear memory | `memory_base + offset` | Offset into linear memory |
| WASM (browser) | Bump/free-list in linear memory | `memory_base + offset` | Offset into linear memory |
| RV32 emulator | Bump/free-list in shared region | `host_ram_base + (addr - shared_start)` | Address in shared region |

### VMContext

VMContext stays per-instance. No changes to VMContext for shared memory.
Shared data is accessed through uniforms holding `ShaderPtr` guest values,
not through VMContext pointers.

```
VMContext (per-instance, unchanged)
┌──────────────────┐
│ fuel: u64        │
│ trap_handler: u32│
│ metadata: *const │
└──────────────────┘

Shared memory (engine-owned, separate)
┌──────────────────────────────┐
│ texture A data               │  ← alloc(width*height*depth)
│ texture B data               │  ← alloc(width*height*depth)
│ ...                          │
└──────────────────────────────┘
```

### RV32 emulator memory layout

The existing emulator gains a third memory region:

```
Address space (32-bit):
  0x00000000  Code (read-only, per-module)
  0x40000000  Shared memory (read-write, engine-owned)
  0x80000000  RAM (read-write, per-instance)
```

One extra `if` branch per memory access. The emulator stays monolithic
(code + RAM + registers in one struct) — no fork or rewrite needed.

### Browser shared memory (fw-wasm, future)

For fw-wasm (firmware running as WASM in browser), shader instances import
the firmware's own `WebAssembly.Memory`. This gives the firmware direct
pointer access to shader data. Security is acceptable because the firmware
is already sandboxed from the main app (lp-studio) via client/server message
passing.

Implementation concerns (shadow stack partitioning, shader stack/heap
regions, builtin memory access) are deferred to the fw-wasm milestone.

### Crate structure

```
lp-shader/
├── lpvm/                   # UPDATE: add alloc/free/ShaderPtr to LpvmEngine trait
├── lpvm-cranelift/         # UPDATE: adapt CraneliftEngine/Module/Instance
├── lpvm-wasm/              # UPDATE: engine creates Memory, instances import it
└── lpvm-emu/               # NEW: EmuEngine/Module/Instance + emu_run.rs (from lpvm-cranelift)

lp-riscv/
└── lp-riscv-emu/           # UPDATE: add shared memory region to Memory struct
```

### Hot path

For this roadmap, `DirectCall` remains a backend-specific optimization.
`lp-engine` uses concrete backend types for the render hot path. The LPVM
trait interface (`call()`) is for filetests and non-hot-path usage.

Future work: abstract texture rendering at the LPVM level with
backend-specific implementations. Potentially compile a `renderTexture`
function into the binary to avoid crossing the shader boundary per-pixel.

## Alternatives Considered

1. **VMContext pointer to shared memory** — Add a `shared_mem: *mut u8`
   field to VMContext, functions reach shared data via indirection. Rejected:
   texture IDs as direct pointers in uniforms is simpler and avoids runtime
   indirection.

2. **Fork/rewrite the rv32 emulator** — Build `emu_v2` with proper
   Module/Instance/Memory separation. Rejected: adding a shared memory region
   to the existing emulator is much simpler and sufficient for filetests.

3. **Module-scoped memory** — Memory owned by the module, shared across
   instances of that module. Rejected: can't share memory across modules
   (needed for cross-shader textures).

4. **Single shared WASM Memory for all instances** (now) — The browser
   memory-sharing approach (shader imports firmware Memory). Deferred: this
   is the right model for fw-wasm but isn't needed until that milestone.

## Risks

1. **ShaderPtr abstraction leaks** — Different backends have fundamentally
   different address models. The `ShaderPtr` dual-pointer abstraction must
   be clean enough that consumers don't need backend-specific code.

2. **Emulator shared memory address conflict** — The shared memory region
   (0x40000000) must not conflict with code or data placed by the linker.
   Need to verify the RV32 linker script leaves this range free.

3. **WASM linear memory sizing** — Starting small and growing works, but
   `memory.grow()` can fail if the host is under memory pressure. Need
   graceful handling.

4. **Trait churn** — M1 changes the trait signatures. M2–M4 implement them.
   M5–M6 migrate consumers. If the trait design is wrong, everything ripples.
   Mitigated by implementing two backends (WASM, Cranelift) before migrating
   consumers.

5. **emu_run.rs extraction** — Moving code from `lpvm-cranelift` to
   `lpvm-emu` changes the dependency graph. `lpvm-emu` depends on
   `lpvm-cranelift` (for RV32 codegen), not the other way around.

## Milestones

### [Milestone 1: Trait redesign](m1-trait-redesign.md)

Add `alloc`/`free`/`ShaderPtr` to `LpvmEngine`. Update `LpvmModule::instantiate`
to accept engine reference. Define `ShaderPtr` type.

### [Milestone 2: WASM shared memory](m2-wasm-shared-memory.md)

Update wasmtime and browser backends to use engine-owned `Memory`. Instances
import shared memory instead of creating their own.

### [Milestone 3: Cranelift JIT update](m3-cranelift-update.md)

Update `CraneliftEngine`/`CraneliftModule`/`CraneliftInstance` for the new
trait signatures.

### [Milestone 4: lpvm-emu](m4-lpvm-emu.md)

Add shared memory region to `lp-riscv-emu`. Create `lpvm-emu` crate with
`EmuEngine`/`EmuModule`/`EmuInstance`. Move `emu_run.rs` from `lpvm-cranelift`.

### [Milestone 5: Migrate filetests](m5-migrate-filetests.md)

Port `lps-filetests` from `GlslExecutable` to LPVM traits. All three
backends.

### [Milestone 6: Migrate engine](m6-migrate-engine.md)

Make `lp-engine` backend-agnostic via LPVM traits. Backend-specific
`DirectCall` for the hot path.

### [Milestone 7: Cleanup](m7-cleanup.md)

Delete obsolete code, remove `GlslExecutable`, verify all builds and tests.
