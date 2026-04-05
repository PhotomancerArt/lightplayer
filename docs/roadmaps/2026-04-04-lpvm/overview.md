# LPVM Roadmap

## Repository state (during renames)

Work on this roadmap may overlap with crate/path renames (`lp-glsl-*` → `lps-*`,
new `lpvm/` tree). **These roadmap documents are the intended target naming.**
If the tree on disk still uses old paths or a find/replace left odd wording
elsewhere, trust this folder and fix drift when you touch files.

## Three layers: `lps` / `lpir` / `lpvm`

Long-term naming groups the shader system into three prefixes:

| Layer | Prefix | Role | Knows about |
|-------|--------|------|-------------|
| **Shader** | `lps-*` | Frontends, logical types, shader-layer tests | vec3, mat4, GLSL (today), WGSL (future) |
| **IR** | `lpir` | Scalarized intermediate representation | `IrType` (F32, I32, Pointer), ops, vregs |
| **VM** | `lpvm-*` | Runtime, execution, backends | Module, Instance, Memory, values, VMContext |

`lps` is not self-explanatory on first read; it is the **LightPlayer shader**
layer, parallel to how `lpir` and `lpvm` abbreviate their domains.

**Dependency direction (allowed edges):**

```
lps-naga ──► lpir          (lowers to LPIR)
lps-naga ──► lps-types     (logical shader types)
lpvm       ──► lpir        (IR module shape, codegen inputs)
lpvm       ──► lps-types   (signatures, metadata consumers see)
lpvm-*     ──► lpvm, lpir  (backends)
lps-filetests ──► lps-*, lpir, lpvm-*   (integration tests)
```

**Important:** `lp-glsl-core`’s `Type` / `FunctionSignature` are **not** LPIR
concepts. LPIR is scalarized; it does not carry logical vec3/mat4 as a first-class
IR type. Those types live in **`lps-types`** (rename of `lp-glsl-core`), not in
`lpir`.

### Target crate map (shader layer)

| Current / transitional | Target (`lps-*`) |
|------------------------|------------------|
| `lp-glsl-core` | `lps-types` |
| `lp-glsl-naga` | `lps-naga` |
| `lp-glsl-builtins` | `lps-builtins` |
| `lp-glsl-builtin-ids` | `lps-builtin-ids` |
| `lp-glsl-filetests` | `lps-filetests` |
| `lp-glsl-diagnostics` | `lps-diagnostics` (or keep name if shared tooling) |

ABI/runtime types from `lp-glsl-abi` and exec traits from `lp-glsl-exec` move
into **`lpvm`**, not into `lps-types`.

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

**Naming cleanup**: retire the `lp-glsl/` catch-all. Use **`lps-*`** for the
shader/language layer, **`lpir`** for scalarized IR only, **`lpvm-*`** for the
runtime. Long-term, the old `lp-glsl` directory name goes away.

## Architecture

```
  GLSL source
       │
       ▼
  lps-naga ──► lpir (IrModule — scalarized)
       │            │
       │            ├──► lpvm-cranelift
       │            ├──► lpvm-rv32
       │            └──► lpvm-wasm
       │
       └── lps-types (LpsType, LpsFunctionSignature — logical types)
                │
                └── referenced by lpvm (metadata / calling convention)
```

## Crate Structure

```
lps/                          # shader layer (may start under lp-glsl/ during migration)
├── lps-types/
├── lps-naga/
├── lps-builtins/
├── lps-builtin-ids/
├── lps-filetests/
└── ...

lp-glsl/lpir/                 # IR crate (path may move to top-level later)
└── ...

lpvm/
├── lpvm/                     # Core: traits, values, vmcontext, layout, runtime metadata
├── lpvm-cranelift/
├── lpvm-rv32/
└── lpvm-wasm/
```

### `lps-types`

Logical shader types: `LpsType`, `LpsFunctionSignature`, `LpsParameter`,
`LpsParamQualifier`. **Zero dependency on `lpir` and `lpvm`.** Shared by
`lps-naga` (frontend) and `lpvm` (runtime metadata and call semantics).

### `lpvm` (core)

Types, traits, and VM/runtime-specific concepts. `no_std + alloc`. Depends on
**`lpir`** (IR module, lowering inputs) and **`lps-types`** (what callers think
functions look like). Replaces **`lp-glsl-abi`** and **`lp-glsl-exec`** (trait
concepts → `LpvmModule` / `LpvmInstance` / `LpvmMemory`).

Contains: `LpvmValue`, `LpvmData`, layout, `LpvmVmContext`, path helpers,
`LpvmModuleMeta`-style metadata (may reference `lps-types` for field types).

### `lpvm-cranelift` / `lpvm-rv32` / `lpvm-wasm`

Backends implementing the LPVM traits. See milestone docs.

## Design Decisions

1. **One `lpvm` core crate** — traits and runtime types stay together; avoids
   circular deps.

2. **Separate backend crates** — different dependency trees (cranelift-\*,
   lp-riscv-\*, wasmtime).

3. **`lps-types` is not `lpir`** — logical shader types are not scalarized IR.
   `lpir` stays free of vec3/mat4 as IR types.

4. **`lpvm` depends on `lps-types` and `lpir`** — runtime needs both “what the
   IR looks like” and “what the user-facing signature is.”

5. **Engine is backend-agnostic** — `lp-engine` depends only on `lpvm` traits,
   generic over `M: LpvmModule`. Firmware picks `lpvm-cranelift` or `lpvm-wasm`.

6. **`Lpvm` prefix on VM-layer external types** — disambiguates from Cranelift,
   wasm, naga. **`Lps` prefix on shader-layer types** in `lps-types`.

7. **`lp-riscv-emu` refactor required** — before `lpvm-rv32` can wrap it cleanly.

8. **`lpvm-interp` deferred** — interpreter stays in `lpir::interp` until needed
   behind LPVM traits.

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

Introduce **`lps-types`** (logical types), **`lpvm`** core crate, wire
dependencies (`lpvm` → `lpir` + `lps-types`). Mechanical refactoring; no new
backends.

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

Port **`lps-filetests`** from `GlslExecutable` to LPVM traits. All three backends.
Primary validation step.

### [M6: Migrate engine](m6-migrate-engine.md)

Make `lp-engine` generic over `LpvmModule`. Production path. End-to-end
validation with firmware builds and `fw-tests`.

### [M7: Cleanup](m7-cleanup.md)

Delete obsolete crates, remove dead code, verify everything builds and passes.
