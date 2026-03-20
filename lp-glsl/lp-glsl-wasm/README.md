# lp-glsl-wasm

GLSL-to-WebAssembly compiler backend. Translates GLSL shader source into
standalone `.wasm` modules that can run in a browser or any WASM runtime.

## Purpose

The primary compilation path for LightPlayer is GLSL → Cranelift IR → native
RISC-V, which runs on the ESP32-C6 firmware. This crate provides a second
backend that targets WASM instead, so the same shaders can run in a web-based
device simulator without requiring Cranelift or a native toolchain.

This backend is not performance-critical. Its job is to produce correct WASM
for interactive preview in a browser. Compilation speed and output code quality
matter less than on the embedded path.

## Architecture

The compiler is a **single-pass tree-walk emitter**. It walks the typed GLSL
AST (produced by `lp-glsl-frontend`) and emits WASM instructions directly,
with no intermediate representation.

```
GLSL source
  → lp-glsl-frontend (parse, type-check, semantic analysis)
  → lp-glsl-wasm (tree-walk codegen)
  → wasm-encoder (binary serialization)
  → .wasm bytes
```

### Why not Cranelift?

Cranelift uses an unstructured CFG (basic blocks + jumps) as its IR. WASM
requires structured control flow (`block`/`loop`/`if`/`end`). Converting from
an unstructured CFG back to structured control flow requires a "relooper" or
"stackifier" algorithm — significant complexity for what is fundamentally a
preview feature.

Since GLSL already has structured control flow (`if`/`for`/`while`, no `goto`),
and WASM also has structured control flow, skipping Cranelift entirely and
walking the AST directly produces a natural 1:1 mapping. The control flow
structure passes through unchanged.

### Why not SPIR-V?

SPIR-V (SSA + structured control flow) would be a good architectural fit, but
no pure-Rust implementation exists. The `no_std` constraint for embedded
compatibility rules out C/C++ dependencies.

## Key design decisions

### Vector scalarization at emission time

GLSL has native vector types (`vec2`, `vec3`, `vec4`). WASM has only scalar
types. The codegen decomposes vector operations into per-component scalar
operations during emission. A `vec3 + vec3` becomes three `f32.add`
instructions operating on individual WASM locals.

This is done inline — there is no scalarization pass. Each emission function
handles the vector-to-scalar decomposition for its specific operation.

### Scratch locals for vector operands

WASM's operand stack is positional: you can't reach past the top to access
buried values. When a vector operation needs to pair up matching components
from two operands (e.g., `a.x + b.x`, `a.y + b.y`), the codegen spills
operands into pre-allocated local variables, then iterates over components
using indexed `local.get`.

Currently these scratch locals are allocated as fixed-size pools per function.
This creates hard limits on how many operands can be live simultaneously — see
"Known limitations" below.

### Dual numeric modes

The backend supports two numeric representations, matching the native backend:

- **Float** — GLSL `float` maps to WASM `f32`. Arithmetic uses native WASM
  float instructions.
- **Q32** — GLSL `float` maps to WASM `i32` in Q16.16 fixed-point. Arithmetic
  is emulated using integer operations with saturation. Complex builtins
  (`sin`, `cos`, `exp`, etc.) are provided as WASM imports from the host.

Q32 mode is the default because it matches the ESP32 firmware path, which
matters for the simulator producing identical results.

### Builtin function strategy

Simple builtins (`abs`, `sign`, `floor`, `clamp`, `smoothstep`, `mix`, `mod`,
`min`, `max`) are **inlined** — the codegen emits the WASM instructions
directly.

Complex builtins (`sin`, `cos`, `atan`, `exp`, `log`, `pow`, `sqrt`,
`normalize`, `length`) are **imported** in Q32 mode — they appear as WASM
imports under the `"builtins"` namespace, and the host provides the
implementations. In Float mode, these use native WASM float intrinsics where
available.

## Module structure

```
src/
  lib.rs              Entry point: glsl_wasm()
  module.rs           WasmModule / WasmExport types
  options.rs          WasmOptions (float mode, error limits)
  types.rs            GLSL type → WASM type mapping
  codegen/
    mod.rs            compile_to_wasm(): module assembly
    context.rs        WasmCodegenContext: per-function state, locals, scratch
    numeric.rs        WasmNumericMode enum
    rvalue.rs         WasmRValue: tracks type of value left on WASM stack
    builtin_scan.rs   Pre-scan for which builtin imports a shader needs
    memory.rs         Linear memory helpers (Q32 builtin ABI)
    stmt/             Statement emission (declarations, loops, if, return)
    expr/             Expression emission:
      mod.rs            emit_rvalue() dispatcher
      binary.rs         Binary ops, vector binary ops, Q32 saturating math
      builtin_inline.rs Inlined builtins (smoothstep, mix, clamp, etc.)
      builtin_call.rs   Imported builtin calls (sin, cos, etc.)
      constructor.rs    Vector constructors (vec2, vec3, vec4)
      component.rs      Swizzle / component access
      variable.rs       Variable load
      literal.rs        Constants
      assignment.rs     Assignment / compound assignment
      ternary.rs        Ternary operator
      lpfx_call.rs      LightPlayer extension function calls
      type_infer.rs     Expression type inference
```

## Known limitations

**Fixed scratch local pools** — Vector operations spill operands into a
pre-allocated block of 8 WASM locals. Three-argument builtins like
`smoothstep(vec3, vec3, vec3)` need 9+ slots and currently fail. A planned
fix replaces the fixed pools with a bump allocator (see
`docs/plans/2026-03-20-wasm-bump-alloc-locals/`).

**No optimization passes** — The tree-walk emitter produces correct but naive
code. There is no dead code elimination, constant folding, or common
subexpression elimination. For the web preview use case this is acceptable.

**No integer genType builtins** — `clamp(ivec3, ...)` and similar integer
vector builtins are not yet supported. Only float genType variants are
implemented.
