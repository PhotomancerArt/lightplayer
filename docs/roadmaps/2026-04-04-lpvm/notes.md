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

Secondary goal: fix naming. The "glsl" prefix on runtime types (`GlslValue`,
`GlslType`, `GlslData`, `GlslExecutable`) conflates the language frontend with
the backend runtime. LPVM provides a clean namespace boundary: everything before
LPIR is "glsl" (language-specific), everything after is "lpvm" (runtime).

## Current State

### Crates that would be affected

- `lp-glsl-abi` — values, types, layout, metadata, VmContext. The "grab bag"
  crate for ABI types. no_std.
- `lp-glsl-exec` — `GlslExecutable` trait. Used by filetests, not by lp-engine.
- `lps-types` — `Type`, `FunctionSignature`, semantic types. no_std.
- `lpir-cranelift` — LPIR → Cranelift → machine code. Also contains RV32
  emulator backend behind `riscv32-emu` feature.
- `lpir` — IR definition + interpreter (`interp` module).
- `lp-glsl-wasm` — LPIR → WASM bytecode emission.
- `lp-riscv-emu` — RISC-V emulator (combines code, memory, thread state).
- `lp-engine` — production runtime. Uses `JitModule`/`DirectCall` from
  lpir-cranelift directly, not `GlslExecutable`.

### Backends today

| Backend                       | Location                                 | Heavy deps              |
|-------------------------------|------------------------------------------|-------------------------|
| Cranelift JIT (host+embedded) | `lpir-cranelift`                         | cranelift-*             |
| RV32 emulator                 | `lpir-cranelift` (feature `riscv32-emu`) | lp-riscv-*              |
| WASM emission                 | `lp-glsl-wasm`                           | wasm-encoder            |
| WASM runner (desktop)         | `lp-glsl-filetests`                      | wasmtime                |
| WASM runner (browser)         | `web-demo`                               | browser WebAssembly API |
| LPIR interpreter              | `lpir::interp`                           | (none beyond lpir)      |

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
Long-term goal is to eliminate `lp-glsl/` entirely — it was the old catchall
for the shader system and is not a good name. `lpir` would also eventually
move out of `lp-glsl/` to its own top-level location (or under `lpvm/`).
The remaining GLSL-specific crates (`lp-glsl-naga`, builtins) may get
reorganized or renamed in a future pass.

## Open Questions

### Q1: Core crate composition

What goes in the `lpvm` core crate?

Candidates: values (`GlslValue`), types (`GlslType`/`Type`), layout functions,
VmContext, Module/Instance/Memory traits, metadata types, data types, path
accessors.

**Context**: Currently these are spread across `lp-glsl-abi` (values, types,
layout, vmcontext, metadata), `lps-types` (Type, FunctionSignature), and
`lp-glsl-exec` (GlslExecutable trait). All are no_std.

**Suggested answer**: All of the above go in one `lpvm` crate. The traits use
the types, so separating them creates circular deps or an awkward third crate.
This matches wasmtime/wasmer where the core crate is the unified API surface.

**Answer**: Yes. One `lpvm` crate containing values, types, layout, vmcontext,
metadata, traits. Replaces `lp-glsl-abi`, `lp-glsl-exec`, absorbs relevant
parts of `lps-types`.

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

**Context**: `lp-glsl-wasm` emits .wasm bytes (no_std, wasm-encoder).
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

### Q5: Relationship between lps-types and lpvm

`lps-types` defines `Type` and `FunctionSignature`. Are these
GLSL-specific or universal runtime types?

**Context**: The types are float, int, vec2-4, mat2-4, bool, arrays, structs.
These aren't GLSL-specific — they're the universal set of shader types. They're
used by the runtime (function metadata, call signatures) as much as by the
compiler.

**Suggested answer**: Move these into `lpvm`. They're runtime types that happen
to also be used by the GLSL frontend. `lp-glsl-naga` would depend on `lpvm`
for type definitions rather than `lps-types`.

**Answer**: These are IR types, not runtime types. They belong in `lpir`, which
is the shared dependency between frontend and backend. `lps-types` gets
absorbed into `lpir`. The frontend (`lp-glsl-naga`) depends on `lpir` to
produce IR — it does not need `lpvm`. `lpvm` depends on `lpir` for type
definitions and adds runtime-specific concepts (Value, VmContext, layout,
traits).

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
level (`fw-esp32` → `lpvm-cranelift`, `fw-wasm` → `lpvm-wasm`).

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
