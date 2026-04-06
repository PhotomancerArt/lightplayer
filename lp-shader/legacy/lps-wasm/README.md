# lps-wasm

GLSL → **WebAssembly** for browser / WASM runtimes. Uses the same **Naga GLSL front end** and
**LPIR** as the device pipeline, then emits WASM with **`wasm-encoder`** (`#![no_std]` + alloc).

## Purpose

The primary LightPlayer path is GLSL → LPIR → **Cranelift** → RISC-V on device. This crate is the
**preview backend**: correct WASM for demos and filetests (`wasm.q32` / float targets) without
pulling Cranelift into the browser build.

## Architecture

```
GLSL source
  → lps-frontend (naga glsl-in → IrModule)
  → lps-wasm::emit (LPIR → WASM instructions)
  → wasm-encoder
  → WasmModule bytes
```

Entry point: `glsl_wasm()` in `lib.rs` (`GlslWasmError` wraps frontend vs codegen failures).

### Why not Cranelift for WASM?

Cranelift’s IR is unstructured CFG-first; WASM wants structured control flow. Bridging that
(relooping / stackification) is a large effort for a non-hot preview path. **LPIR** already reflects
structured GLSL control flow, so lowering LPIR → WASM directly stays aligned with the language and
with the IR shared with `lpvm-cranelift`.

### Why not SPIR-V?

No mature pure-Rust SPIR-V consumer that fits the same `no_std`-friendly constraints as the rest of
the shader stack; Naga is the chosen front end.

## Key design decisions

### Vector scalarization at emission time

WASM is scalar-typed. Vector ops are lowered to per-lane locals and scalar instructions in the
`emit/` code (no separate scalarization pass).

### Scratch locals

The stack model sometimes spills vector operands to locals. Fixed scratch pools per function impose
limits on some vector-heavy builtins — see **Known limitations**.

### Float vs Q32

- **Float** — GLSL `float` as WASM `f32`, native float ops where used.
- **Q32** — `float` as Q16.16 `i32`; math uses integer/saturation patterns; heavy builtins are
  **imports** from the host (`builtins` module), matching firmware behavior for the web demo.

### Builtins

Simple ops are inlined in the emitter; complex Q32 builtins match the import ABI described in
generated `emit/builtin_wasm_import_types.rs` (from `lps-builtins-gen-app`).

## Module layout

```
src/
  lib.rs                         glsl_wasm(), error types, re-exports
  module.rs                      WasmModule, exports, shadow stack hook
  options.rs                     WasmOptions
  emit/
    mod.rs                       module / function assembly
    func.rs                      per-function emission
    control.rs                   structured control flow
    ops.rs                       arithmetic / compares
    q32.rs                       Q32 helpers
    memory.rs                      linear memory / ABI helpers
    imports.rs                   WASM imports
    builtin_wasm_import_types.rs AUTO-GENERATED (do not edit)
```

## Known limitations

**Fixed scratch local pools** — Some multi-vector builtins can exceed current local pools. See
`docs/plans-done/2026-03-20-wasm-bump-alloc-locals/00-plan.md` for the planned bump-allocator
approach.

**Limited optimization** — Straightforward lowering; acceptable for preview.

**Integer genType builtins** — Not all `ivec`/`uvec` builtin overloads are implemented; float
genTypes are the main focus.
