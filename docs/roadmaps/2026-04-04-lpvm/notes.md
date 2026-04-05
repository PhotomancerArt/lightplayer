# Notes

## Scope and Goals

LPVM is the runtime system for executing compiled LPIR modules. The goal is to
cleanly separate compiled code, memory, and execution state — concepts currently
tangled in `GlslExecutable`, `Riscv32Emulator`, and `JitModule`.

Inspired by WASM's Module/Instance/Memory separation, LPVM introduces:

- **Module** — compiled code, immutable after compilation
- **Instance** — running execution context (VMContext, call state)
- **Memory** — linear memory backing the instance

This separation enables: multiple instances from one module, clean VMContext
ownership, future parallelism, and a uniform API across backends.

Secondary goal: fix naming. Retire the `lp-shader/` catch-all in favor of three
layers: **`lps-*`** (shader / logical types / frontends), **`lpir`** (scalarized
IR only), **`lpvm-*`** (runtime). Runtime types (`GlslValue`, …) move to `lpvm`;
logical signatures live in **`lps-shared`**, not in `lpir` (LPIR does not carry
vec3/mat4 as IR types).

## Current State

### Crates that would be affected

- `lpvm` → **`lpvm`** — values, layout, runtime metadata, VmContext.
- `lps-exec` → **`lpvm`** traits — `GlslExecutable` replaced by LPVM traits.
- `lps-core` → **`lps-shared`** — logical `LpsType`, `LpsFunctionSignature`, etc.
- `lpir-cranelift` — splits into **`lpvm-cranelift`** + **`lpvm-rv32`** (object/emu).
- `lpir` — scalarized IR + interpreter only.
- `lps-wasm` → **`lpvm-wasm`**.
- `lp-riscv-emu` — refactor for module/memory/instance; still general-purpose.
- `lp-engine` — becomes generic over `LpvmModule`; today uses `JitModule`/`DirectCall`.

### Backends today

| Backend                       | Location                                  | Heavy deps              |
|-------------------------------|-------------------------------------------|-------------------------|
| Cranelift JIT (host+embedded) | `lpir-cranelift`                          | cranelift-*             |
| RV32 emulator                 | `lpir-cranelift` (feature `riscv32-emu`)  | lp-riscv-*              |
| WASM emission                 | `lps-wasm`                                | wasm-encoder            |
| WASM runner (desktop)         | `lps-filetests` (transitional names vary) | wasmtime                |
| WASM runner (browser)         | `web-demo`                                | browser WebAssembly API |
| LPIR interpreter              | `lpir::interp`                            | (none beyond lpir)      |

## Motivation

The immediate driver is separating compiled code from runtime state to enable
VMContext (globals, uniforms, fuel). But the larger motivation is `fw-wasm` —
running LightPlayer in-browser as a development simulation target. This
requires the engine to be backend-agnostic: Cranelift JIT on ESP32/desktop,
browser WebAssembly API in-browser. LPVM enables this by abstracting the
runtime behind traits.

`fw-wasm` is not what we're building right now — it's what we're enabling.
But its needs (especially: no virtual dispatch on the hot path, backend
portability) directly influence the LPVM trait design.

## Constraints

- All core types and traits must be `no_std + alloc` (embedded target).
- The Cranelift JIT backend must remain available on embedded without `std`.
- Backend dependency trees are very different — cranelift, wasmtime, riscv-emu
  are all heavy and independent.
- Migration must be incremental — can't rewrite everything at once.

### Performance

The hot path is: calling LPVM functions and resetting global state. To a lesser
extent: creating instances and setting uniforms. This must be as fast as
possible across different backends. Generics (monomorphization) preferred over
virtual dispatch — each firmware crate selects its backend at the top level.

### Filetests

Filetests are fundamental to system validation. Performance isn't a primary
concern there, but a clear, consistent, semantic API that enables good tests is.
Filetests and the engine should share as much of the code path as possible, so
extensive tests exercise the real pipeline. It's understood that
easy-in-tests-API and absolute-maximum-performance are sometimes at odds — we
make reasonable concessions.

### Q10: Directory structure

**Answer**: `lpvm/` at the repo root, alongside `lp-riscv/`, `lp-core/`, etc.
Long-term: eliminate **`lp-shader/`** as a directory name. Shader layer becomes
**`lps-*`** (`lps-shared`, `lps-frontend`, `lps-builtins`, `lps-filetests`, …).
**`lpir`** may move to top-level. **`lpvm/`** holds VM crates. Repo may be
mid-rename; [overview.md](overview.md) is canonical for target names.

## Open Questions

### Q1: Core crate composition

What goes in the `lpvm` core crate?

Candidates: values (`GlslValue` → `LpvmValue`), runtime metadata/layout (`GlslType`
ABI side → `Lpvm*` types that may reference **`lps-shared`**), layout functions,
VmContext, Module/Instance/Memory traits, `LpvmData`, path accessors.

**Context**: Spread across `lpvm`, **`lps-shared`** (logical signatures),
and `lps-exec` (trait).

**Answer**: One **`lpvm`** crate for **VM/runtime** surface: traits, `LpvmValue`,
layout, VMContext, runtime-oriented metadata. **Logical** shader types
(`LpsType`, `LpsFunctionSignature`, …) stay in **`lps-shared`** — they are not
LPIR (scalarized) and not VM-only; frontend and runtime both use them.
**`lpvm`** depends on **`lps-shared`** and **`lpir`**. Replaces `lpvm` and
`lps-exec`; does **not** absorb `lps-shared`.

### Q2: Backend crate organization

Should backends be separate crates or feature-gated in one crate?

**Context**: Backend deps are wildly different (cranelift-*, lp-riscv-*,
wasmtime, wasm-encoder, nothing). On embedded, only cranelift is needed.

**Suggested answer**: Separate crates. Feature flags for fundamentally different
dependency trees leads to complexity. Separate crates let each consumer pull
exactly what it needs.

**Answer**: Separate crates. `lpvm-cranelift`, `lpvm-rv32`, `lpvm-wasm`,
`lpvm-interp`.

### Q3: WASM backend structure

WASM has two runtime paths: wasmtime (desktop/CI) and browser WebAssembly API.
How should the emission vs runtime split work?

**Context**: `lps-wasm` emits .wasm bytes (no_std, wasm-encoder).
Filetests run those bytes via wasmtime. web-demo hands them to the browser.
The browser path has no Rust runtime — the browser IS the runtime.

**Suggested answer**: One `lpvm-wasm` crate for emission. wasmtime runner is
either a feature flag on lpvm-wasm or a thin separate crate. Browser path
doesn't need a Rust crate (web-demo just calls emission).

**Answer**: One `lpvm-wasm` crate. Core is emission (LPIR → .wasm bytes),
`no_std + alloc`. A `runtime` feature adds Module/Instance trait
implementations, with the backing runtime auto-selected by target arch:
`wasmtime` on native, `wasm-bindgen` + browser WebAssembly API on `wasm32`.
No manual feature flag needed for the native/browser split.

### Q4: LPIR interpreter placement

Should the interpreter stay in `lpir` or move to `lpvm-interp`?

**Context**: `lpir::interp` is tightly coupled to IR ops (it switches on every
op type). It's used for unit testing LPIR and in naga integration tests. It
doesn't implement `GlslExecutable` today.

**Suggested answer**: Keep the core interpret loop in `lpir`. Create a thin
`lpvm-interp` that wraps it with Module/Instance trait implementations. This
keeps `lpir` self-contained for IR-level testing.

**Answer**: Not needed right now. The interpreter stays in `lpir::interp` for
IR-level testing. `lpvm-interp` is future work if we ever need the interpreter
behind the LPVM trait interface.

### Q5: Relationship between `lps-shared`, `lpir`, and `lpvm`

`lps-shared` holds logical shader types (`LpsType`, `LpsFunctionSignature`, …).
Are these GLSL-only? IR? Runtime-only?

**Context**: float, vec2–4, mat2–4, bool, arrays, structs — shared by GLSL
today and other shading languages later. **`lpir` is scalarized** (no vec3 as an
IR type). The frontend lowers to LPIR but still emits **metadata** using logical
types.

**Answer**: **`lps-shared`** is the **shader-layer type vocabulary** — shared by
**`lps-frontend`** (and future frontends) and **`lpvm`** (signatures, calling
convention, metadata). **`lpir`** does **not** absorb these types. **`lpvm`**
adds runtime values (`LpvmValue`), VMContext, layout, and traits; it **depends on**
**`lps-shared`** where signatures matter. Naming: **`Lps*`** in `lps-shared`,
**`Lpvm*`** for VM-layer runtime types.

### Q6: What happens to lp-riscv-emu?

The emulator currently combines code, memory, and thread state.
Does it get refactored to fit Module/Instance/Memory, or does `lpvm-rv32`
just wrap it?

**Context**: `Riscv32Emulator` owns registers, PC, memory, instruction count,
traps, serial, timing. It's used beyond LPVM (ELF loading, guest tests, fw-tests).

**Suggested answer**: `lp-riscv-emu` stays as-is — it's a general-purpose RV32
emulator with its own consumers. `lpvm-rv32` wraps it, implementing
Module/Instance/Memory by mapping onto emulator APIs. The emulator doesn't need
to know about LPVM.

**Answer**: `lp-riscv-emu` needs to be refactored (or rewritten) to support
the Module/Memory/Instance separation. Its current model combines code, memory,
and thread state into one struct, which fundamentally doesn't fit. This
refactor is a prerequisite milestone before building `lpvm-rv32`. The emulator
needs to separate compiled code from memory from execution state at its own
level, then `lpvm-rv32` wraps it with the LPVM trait interface.

### Q7: How does lp-engine consume lpvm?

Does lp-engine depend on specific backends or just the core traits?

**Context**: Today lp-engine directly uses `lpir_cranelift::jit()`,
`JitModule`, and `DirectCall`. It doesn't go through `GlslExecutable`.

**Suggested answer**: lp-engine depends on `lpvm` (core traits) + `lpvm-cranelift`
(concrete backend). The engine needs the concrete JIT for performance-critical
direct calls. Trait objects may be too expensive for the hot render path on
embedded.

**Answer**: lp-engine MUST depend only on `lpvm` (core traits), not on any
specific backend. The high-level goal includes `fw-wasm` — running LightPlayer
in-browser as a development simulation target. In-browser, shaders run via
`lpvm-wasm` (browser WebAssembly API), not Cranelift. So the engine must be
backend-agnostic. Generics (monomorphization) over `M: lpvm::Module` give
zero-cost abstraction: each firmware crate selects its backend at the top
level (`fw-esp32` → `lpvm-cranelift`, `fw-wasm` → `lpvm-wasm`). Traits are
`LpvmModule` / `LpvmInstance` / `LpvmMemory` (names per M1).

### Q8: Naming conventions

Should LPVM types drop all prefixes (just `Value`, `Type`, `Module`) or use
`Lpvm` prefix (`LpvmValue`, `LpvmModule`)?

**Context**: Within the `lpvm` crate, bare names work (`lpvm::Value`). But
consumers import these alongside other types. `Value` and `Type` are extremely
common names that will collide.

**Suggested answer**: TBD — need to weigh ergonomics vs collision risk.

**Answer**: Use `Lpvm` prefix on externally-facing types: `LpvmModule`,
`LpvmInstance`, `LpvmMemory`, `LpvmValue`, `LpvmData`, etc. Given the number
of ecosystems wired together (Naga, Cranelift, WASM, LPIR), collisions are
inevitable without prefixes. "Lpvm" means "runtime thing" and provides clear
disambiguation — especially inside backend crates like `lpvm-cranelift` where
both LPVM's `Value` and Cranelift's `Value` coexist. Internal/private types
that aren't part of the cross-crate API can use shorter generic names.
