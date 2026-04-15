# lpir

**LightPlayer Intermediate Representation** — a flat, scalarized IR with structured control flow,
designed so the compiler speaks its own language instead of any specific backend's.

Full specification: [`docs/lpir/`](../../docs/lpir/) (overview, types, ops, control flow, calls,
imports, text format, GLSL mapping, future).

## Why another IR?

LightPlayer compiles GLSL to native RISC-V on an ESP32 microcontroller at runtime. If the
compiler's internal representation *is* a backend IR, every part of the stack — frontend lowering,
builtins, vector decomposition, tests — depends on that backend's types and calling conventions.
Backend changes break everything. Testing the compiler means running the backend.

LPIR is an **anti-corruption layer** (sometimes called a *Ports and Adapters* or *Hexagonal
Architecture* boundary). It lets the compiler core — parsing, type checking, scalarization, builtin
decomposition — be written entirely in LightPlayer's own terms. Backends only appear behind
adapter crates: `lpvm-native` (custom RV32 codegen), `lpvm-cranelift` (Cranelift codegen), and
`lpvm-wasm` (WebAssembly emission). The same IR also feeds an in-process interpreter
(`lpir::interp`) and any future backend.

Concretely, this gives us:

- **Decoupled testing.** The interpreter runs any LPIR program without any codegen backend.
  Filetests can verify scalarization, control flow, builtins, and GLSL semantics using the
  interpreter alone.
- **Multiple backends from one lowering.** `lps-frontend` lowers GLSL once; four consumers
  (native / Cranelift / WASM / interpreter) share the result.
- **Stable compiler internals.** Backend version bumps, ABI changes, or ISA feature flags stay
  behind their adapter boundary and do not ripple into the frontend or tests.

## What LPIR looks like

LPIR is intentionally simple. Two scalar types (`f32`, `i32`), virtual registers, structured
control flow, and module-qualified imports for builtins.

A basic function:

```
func @lerp(v0:f32, v1:f32, v2:f32) -> f32 {
  v3:f32 = fconst.f32 1.0
  v4:f32 = fsub v3, v2
  v5:f32 = fmul v0, v4
  v6:f32 = fmul v1, v2
  v7:f32 = fadd v5, v6
  return v7
}
```

Control flow is structured (no CFG, no basic blocks):

```
func @abs(v0:f32) -> f32 {
  v1:f32 = fconst.f32 0.0
  v2:i32 = flt v0, v1
  if v2 {
    v0 = fneg v0
  }
  return v0
}
```

A loop with a guard:

```
func @sum_to_n(v0:i32) -> i32 {
  v1:i32 = iconst.i32 0
  v2:i32 = iconst.i32 0
  loop {
    v3:i32 = ilt_s v2, v0
    br_if_not v3
    v1 = iadd v1, v2
    v4:i32 = iconst.i32 1
    v2 = iadd v2, v4
    continue
  }
  return v1
}
```

Builtins are module-qualified imports — the set is open-ended without changing the IR:

```
import @std.math::fsin(f32) -> f32
import @lpfx::noise3(i32, i32, i32, i32) -> (i32, i32, i32)

func @example(v0:f32) -> f32 {
  v1:f32 = call @std.math::fsin(v0)
  return v1
}
```

## Design choices

| Choice                      | Why                                                                                                                                                 |
|-----------------------------|-----------------------------------------------------------------------------------------------------------------------------------------------------|
| **Flat / ANF**              | Every intermediate is a named VReg — no expression trees. Eliminates the scratch-local aliasing bugs that plagued the old single-pass WASM emitter. |
| **Scalarized (v1)**         | Vectors decompose to scalar VRegs during lowering. Keeps backend complexity low; SIMD extensions are a future direction.                            |
| **Non-SSA**                 | VRegs can be reassigned. Lowering stays simple; backends that want SSA rebuild it themselves.                                                       |
| **Structured control flow** | `if`/`loop`/`switch`/`break`/`continue` — mirrors GLSL and maps directly to WASM. Other backends lower structured CF to their own CFG trivially.   |
| **Float-mode-agnostic**     | `fadd` means "GLSL float add". Whether that becomes IEEE `f32` or Q16.16 fixed-point is a backend decision, not an IR property.                     |
| **Open import modules**     | `@std.math::fsin`, `@lp.q32::…`, `@lpfx::…` — adding builtins never changes the opcode set.                                                         |

## Pipeline

```
GLSL source
  │
  ▼
lps-frontend  (Naga glsl-in → IrModule)
  │
  ├──► lpvm-native       (custom RV32 codegen — default on-device JIT)
  ├──► lpvm-cranelift     (Cranelift → RISC-V / host JIT)
  ├──► lpvm-wasm          (wasm-encoder → .wasm)
  └──► lpir::interp       (in-process interpreter, testing)
```

Lowering is **mode-unaware** (no f32-vs-Q32 in the IR). Backends are **mode-aware** and apply the
chosen numeric strategy during emission.

## Crate contents

```
src/
  lib.rs           public API, re-exports
  types.rs         IrType, VReg, SlotId, FloatMode, CalleeRef
  op.rs            Op enum (flat instruction stream)
  module.rs        IrModule, IrFunction, ImportDecl, SlotDecl
  builder.rs       ModuleBuilder / FunctionBuilder
  print.rs         IrModule → text format
  parse.rs         text format → IrModule
  validate.rs      structural validation
  interp.rs        tree-walking interpreter (ImportHandler trait)
  glsl_metadata.rs GLSL type/param metadata carried alongside IrModule
  tests/           roundtrip, interpreter, validation tests
```

`#![no_std]` + alloc. No backend dependency. No GLSL parser dependency.

## Documentation

The full language spec lives in [`docs/lpir/`](../../docs/lpir/):

| Doc                                                         | Contents                                                                    |
|-------------------------------------------------------------|-----------------------------------------------------------------------------|
| [00-overview](../../docs/lpir/00-overview.md)               | Rationale, pipeline, IR classification, design decisions, numeric semantics |
| [01-types-and-vregs](../../docs/lpir/01-types-and-vregs.md) | `f32` / `i32`, VReg naming and rules                                        |
| [02-core-ops](../../docs/lpir/02-core-ops.md)               | Arithmetic, comparison, logic, constants, casts, select/copy                |
| [03-memory](../../docs/lpir/03-memory.md)                   | Slots, load/store, memcpy, pointer model                                    |
| [04-control-flow](../../docs/lpir/04-control-flow.md)       | if/else, loop, switch, break/continue, br_if_not                            |
| [05-calls](../../docs/lpir/05-calls.md)                     | Function declarations, call op, multi-return, recursion                     |
| [06-import-modules](../../docs/lpir/06-import-modules.md)   | `@std.math`, `@lp.q32`, `@lpfx` modules                                     |
| [07-text-format](../../docs/lpir/07-text-format.md)         | Lexical rules, EBNF grammar, well-formedness                                |
| [08-glsl-mapping](../../docs/lpir/08-glsl-mapping.md)       | Naga expression/statement → LPIR lowering tables                            |
| [09-future](../../docs/lpir/09-future.md)                   | Vector types, i64, optimizations, diagnostics                               |
