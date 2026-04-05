# LPVM Roadmap

## What is LPVM?

LPVM is the runtime system for executing compiled LPIR modules. It introduces
a clean separation between compiled code (Module), execution state (Instance),
and linear memory (Memory) — concepts currently tangled across `GlslExecutable`,
`Riscv32Emulator`, and `JitModule`.

## Why

**Immediate need**: VMContext for globals, uniforms, and fuel requires clean
ownership of per-instance state, which the current architecture doesn't support.

**Larger motivation**: `fw-wasm` — running LightPlayer in-browser as a
development simulation target. The engine must be backend-agnostic: Cranelift
JIT on ESP32/desktop, browser WebAssembly API in-browser. LPVM enables this by
abstracting the runtime behind traits that are monomorphized per firmware target.

**Naming cleanup**: the "glsl" prefix on runtime types (`GlslValue`, `GlslType`,
`GlslExecutable`) conflates the language frontend with the backend runtime.
LPVM draws a clean boundary: everything before LPIR is "glsl" (language),
everything after is "lpvm" (runtime). Long-term, `lp-glsl/` goes away entirely.

## Architecture

```
  GLSL source
       │
       ▼
  lp-glsl-naga ──► lpir (IrModule, Type, FunctionSignature)
                      │
          ┌───────────┼───────────┐
          ▼           ▼           ▼
   lpvm-cranelift  lpvm-rv32  lpvm-wasm
          │           │           │
          └─────┬─────┘     ┌────┘
                ▼            ▼
          LpvmModule    LpvmModule
          LpvmInstance  LpvmInstance
          LpvmMemory    LpvmMemory
                │            │
                ▼            ▼
            lp-engine<M: LpvmModule>
                │
       ┌────────┼────────┐
       ▼        ▼        ▼
   fw-esp32  fw-emu   fw-wasm (future)
```

## Crate Structure

```
lpvm/
├── lpvm/                # Core: traits, values, vmcontext, layout, metadata
├── lpvm-cranelift/      # Cranelift JIT backend
├── lpvm-rv32/           # RV32 emulator backend
└── lpvm-wasm/           # WASM emission + runtime (wasmtime / browser)
```

### `lpvm` (core)

Types, traits, and runtime-specific concepts. `no_std + alloc`. Depends on
`lpir` for type definitions (`Type`, `FunctionSignature`). Replaces
`lp-glsl-abi`, `lp-glsl-exec`, absorbs runtime-relevant parts of
`lps-types`.

Contains: `LpvmModule`, `LpvmInstance`, `LpvmMemory` traits. `LpvmValue`,
`LpvmData`, layout functions, `VmContext`, module metadata.

### `lpvm-cranelift`

Cranelift JIT backend. Implements `LpvmModule`/`LpvmInstance`/`LpvmMemory`.
Depends on `lpvm`, `lpir`, `cranelift-*`. Works on both host (native ISA)
and embedded (RISC-V) without `std`.

### `lpvm-rv32`

RV32 emulator backend. Wraps `lp-riscv-emu` with LPVM trait interface.
Depends on `lpvm`, `lpir`, `lp-riscv-*`. Requires prior refactor of
`lp-riscv-emu` to support Module/Memory/Instance separation.

### `lpvm-wasm`

WASM emission (LPIR → `.wasm` bytes) as core, `no_std + alloc`. A `runtime`
feature adds `LpvmModule`/`LpvmInstance` implementations, with the backing
runtime auto-selected by target: `wasmtime` on native, browser `WebAssembly`
API on `wasm32`.

## Design Decisions

1. **One core crate** — traits use types; splitting them creates circular deps
   or an awkward third crate. Matches wasmtime/wasmer prior art.

2. **Separate backend crates** — dependency trees are too different for feature
   flags (cranelift-\*, lp-riscv-\*, wasmtime, wasm-encoder). Each consumer
   pulls exactly what it needs.

3. **IR types live in `lpir`** — `Type` and `FunctionSignature` are IR concepts,
   not runtime concepts. The frontend depends on `lpir`, not `lpvm`. `lpvm`
   depends on `lpir` for type definitions. `lps-types` is absorbed into
   `lpir`.

4. **Engine is backend-agnostic** — `lp-engine` depends only on `lpvm` traits,
   generic over `M: LpvmModule`. Monomorphized per firmware target for
   zero-cost abstraction. Enables `fw-wasm`.

5. **`Lpvm` prefix on external types** — too many ecosystems (Naga, Cranelift,
   WASM, LPIR) for bare names. `LpvmModule`, `LpvmValue`, etc. Internal types
   can use shorter names.

6. **`lp-riscv-emu` refactor required** — its current model combines code,
   memory, and thread state. Must support the separation at its own level
   before `lpvm-rv32` can wrap it.

7. **`lpvm-interp` deferred** — interpreter stays in `lpir::interp` for
   IR-level testing. Future work if needed behind LPVM traits.

## Constraints

- All core types and traits: `no_std + alloc` (ESP32 target).
- Cranelift JIT backend available on embedded without `std`.
- Migration must be incremental.
- Hot path (function calls, global state reset) must be as fast as possible —
  generics over trait objects.
- Filetests and engine share code paths where possible — tests exercise the
  real pipeline.
- Easy-in-tests and maximum-performance are sometimes at odds; reasonable
  concessions accepted.

## Milestones

### Ordering rationale

- In-place refactors first — renames and moves are low-risk and mechanical.
  Get them done before building new things on top.
- Build new system alongside old — avoid breaking the whole build. Old and new
  coexist until consumers are migrated.
- WASM first — it has the strictest model (we don't control the WASM runtime)
  and the least flexibility. Let it drive the API design.
- Cranelift second — validates the API with a backend we control. Should be
  mostly thin wrappers. Surfaces any kinks or missing pieces in the traits.
- RV32 third — requires the emulator refactor, which is the hardest backend
  work. By this point the API is stable.
- Migrate consumers last — filetests first (they validate everything), then
  engine (production path).
- Delete old code only after everything is migrated and passing.

### [M1: Renames, moves, and new types](m1-renames-moves-new-types.md)

Create `lpvm` core crate, absorb `lps-types` into `lpir`, move types from
`lp-glsl-abi`/`lp-glsl-exec`. Mechanical refactoring, no new backends.

### [M2: `lpvm-wasm`](m2-lpvm-wasm.md)

Build the WASM backend first — strictest model drives the API. Emission +
wasmtime runtime. Unit tests.

### [M3: `lpvm-cranelift`](m3-lpvm-cranelift.md)

Build the Cranelift JIT backend. Validates the API with a second backend.
Must work on `riscv32imac-unknown-none-elf` without `std`.

### [M4: `lpvm-rv32`](m4-lpvm-rv32.md)

Refactor `lp-riscv-emu` for Module/Memory/Instance separation, then build the
LPVM wrapper. Hardest backend milestone.

### [M5: Migrate filetests](m5-migrate-filetests.md)

Port filetests from `GlslExecutable` to LPVM traits. All three backends.
Primary validation step.

### [M6: Migrate engine](m6-migrate-engine.md)

Make `lp-engine` generic over `LpvmModule`. Production path. End-to-end
validation with firmware builds and `fw-tests`.

### [M7: Cleanup](m7-cleanup.md)

Delete old crates, remove dead code, verify everything builds and passes.
