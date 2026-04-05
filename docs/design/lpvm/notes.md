# LPVM - LightPlayer Virtual Machine

**Roadmap (milestones, three-layer `lps` / `lpir` / `lpvm` naming):**
[docs/roadmaps/2026-04-04-lpvm/overview.md](../../roadmaps/2026-04-04-lpvm/overview.md)

This document contains notes about a major refactor to separate the existing `GlslExecutable`
into `LpvmModule`, `LpvmInstance`, and `LpvmMemory` concepts.

These ideas are inspired by WASM's `Module`, `Instance`, and `Memory` concepts.

# Justification

The existing `GlslExecutable` and related `Riscv32Emulator` implementation combine complied code,
memory, and thread state into a single concept.

When introducing a VMContext concept for global variables, unifoms, fuel, and other thread-specific
state, it was hard to find a clear way to architect this idea.

The realization was made that because of `Riscv32Emulator` combining the above concepts, the
only place to put the VMContext was in the `GlslExecutable` itself, which is not in line with
the future goals for parallelism.

Additonally, this restriction is arbitray. There is no fundtamental reason why `Riscv32Emulator`
should be limited in this way, other than history.

# Implementation

LPVM will be implemented as a new set of crates, under the `lpvm/` directory. Code will be copied
from the existing crates as needed, and we will then migrate consumers to use the new crates.

# Nomenclature

Historically, we have used the "glsl" prefix for most concepts in the compiler and runtime.

This doesn't well separate compiler from runtime, and mixes frontend and backend concepts:
glsl is a language, independent of the runtime. Future frontends like WGSL would make this
confusing.

# Scope

LPVM is the runtime system for executing compiled LPIR. There are four main ways that LPIR
can be executed:

- JIT compilation to machine code using cranelift
- RV32 emulation using `riscv32-emu`
- WASM compilation using `wasmtime`
- Directly interpreted LPIR using `lpir::interp`

# Core Traits

Inspired by WASM's `Module`, `Instance`, and `Memory` concepts, the core traits for LPVM are:

- `LpvmMemory` -- represents the linear memory of the VM
- `LpvmModule` -- represents a compiled LPIR module
- `LpvmInstance` -- represents a running instance of a LPIR module

# Crates
