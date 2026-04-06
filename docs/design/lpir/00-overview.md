# LPIR overview

This spec defines LightPlayer Intermediate Representation (LPIR), its role in the compiler pipeline,
the main design choices behind it, and alternatives that were rejected or deferred.

## What LPIR is

LPIR (LightPlayer Intermediate Representation) is a flat, scalarized, non-SSA intermediate
representation with structured control flow and virtual registers. It sits between the Naga-based
GLSL frontend and target-specific backends (Cranelift for native RISC-V / host JIT, WebAssembly for
browser preview, and an in-crate interpreter for testing).

## Source language

The input to Naga → LPIR lowering is **GLSL 4.50 core** (`#version 450 core`).

## Rationale

LPIR addresses two concrete issues in the prior pipeline:

1. **Scratch local aliasing** — The WebAssembly backend emitted code in a single pass from Naga’s
   expression arena, interleaving vector scalarization, temporary local allocation, and byte
   emission. That coupling produced incorrect reuse of scratch locals for nested expressions. The
   failure mode appeared with both the legacy frontend and Naga.

2. **Duplicated middle-end work** — Multiple backends each implemented vector scalarization, builtin
   decomposition, LPFX handling, and GLSL-to-target details independently.

LPIR mitigates (1) by binding every intermediate to a distinct virtual register, which maps
one-to-one to a target local or value at emission time. It mitigates (2) by centralizing
scalarization and builtin decomposition in a single Naga → LPIR lowering step shared by all
backends.

## Pipeline

```
GLSL source
  │
  ▼
Naga frontend (parse + type check)
  │
  ▼
naga::Module (expression arena + statements)
  │
  ▼  Naga → LPIR lowering (mode-unaware, shared middle-end)
  │   • walks Naga expression arena + statements
  │   • scalarizes vectors: vec3 add → 3× fadd
  │   • decomposes builtins: smoothstep → scalar math
  │   • handles LPFX calls and out-pointer ABI
  │   • VReg allocator: monotonic counter
  │
  ▼
IrFunction { body: Vec<Op>, vreg_count, vreg_types }
  │
  ┌──────────┼──────────┐
  ▼          ▼          ▼
Cranelift   WASM       Interpreter
emitter    emitter    (lpir::interp)
  ▼          ▼          ▼
machine    .wasm      test results
code       bytes
```

Lowering is **mode-unaware**: it does not encode f32 vs fixed-point (Q32) choice. Emitters are *
*mode-aware**: they interpret the same LPIR under the selected numeric mode. The interpreter (
`lpir::interp`) runs LPIR directly for testing without invoking any backend.

## IR classification

| Property                      | Description                                                                                                                                                                                                                                      |
|-------------------------------|--------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| Flat / ANF-style              | Every intermediate value is bound to a named virtual register. There are no expression trees in the IR.                                                                                                                                          |
| Scalarized                    | Version 1 has no vector types in the IR. Vector operations are lowered to sequences of scalar ops. A future revision may add SIMD-oriented representations.                                                                                      |
| Non-SSA                       | Virtual registers may be reassigned. Each target performs its own SSA construction or equivalent as needed.                                                                                                                                      |
| Structured control flow       | `if`/`else`, loops, `break`, `continue`, `br_if_not`, `switch`, `return`. There is no explicit CFG or basic-block graph. This aligns with WebAssembly structured control; lowering structured control to a CFG for Cranelift is straightforward. |
| Float-mode-agnostic           | Operations such as `fadd` and `fmul` express GLSL floating-point semantics. Whether those map to IEEE f32 or to Q32 fixed-point is an emitter parameter, not part of the IR text.                                                                |
| Width-aware virtual registers | Each virtual register has a concrete type: `f32`, `i32`, or `ptr` (pointer-sized address). Boolean conditions use `i32` (zero vs nonzero).                                                                                                       |
| Scalar types (v1)             | `f32` (IEEE 754), `i32` (signedness defined per operation), and `ptr` (opaque addresses: 32-bit on RV32/WASM, native pointer width on host JIT). No dedicated `bool` type, no general `i64` data type, no vectors in v1. See design decision 3.  |

## Key design decisions

1. **Width-aware virtual register types and short op names** — Types are explicit on registers;
   opcode names stay compact and stable across backends.

2. **Q32 in the emitter, not as an LPIR→LPIR rewrite** — Fixed-point behavior is applied when
   emitting target code. Backends differ: for example, WebAssembly may use inline `i64` arithmetic
   where appropriate; Cranelift may use saturating paths via builtin calls or an all-`i32` wrapping
   strategy. A shared IR-level Q32 pass would not match these per-backend choices. **Concrete Q32
   numeric rules** (builtins, div0, `isnan`/`isinf`, comparisons) are defined in [
   `../q32.md`](../q32.md); that document is normative for implementations.

3. **Pointer model via `ptr`** — Opaque addresses (stack slots, GLSL `out` / `inout` / pointer
   parameters, VM context) use the `ptr` type so emitters can map it to the target’s pointer width (
   32-bit on RV32 and WASM32, full width on 64-bit host JIT). Integer `i32` remains for numeric work
   and small offsets; `load` / `store` / `memcpy` take a `ptr` base (or a `ptr` produced by address
   arithmetic). WASM emission maps `ptr` to linear-memory `i32`; RV32 uses 32-bit pointers.

4. **Module-qualified imports for external functions** — Math builtins, Q32 helpers, LPFX, Lygia,
   and similar capabilities are not a closed opcode enum. They appear as
   `import @module::name(...)` (exact surface syntax is defined in the text-format specification).
   Emitter configuration supplies implementations **per module name**, so the set of importable
   modules is open-ended.

5. **Single `call` operation** — Ordinary and imported calls share one call shape; distinctions are
   carried by the callee (name, signature, import metadata), not by parallel opcode families.

6. **Non-SSA** — Keeps lowering simple and matches targets that rebuild SSA internally. Reassignment
   is explicit in the IR.

7. **Structured control flow** — Matches WebAssembly constraints and avoids maintaining a parallel
   CFG in the middle-end.

8. **Scalarized IR (v1)** — Reduces backend complexity and matches the primary WebAssembly path.
   SIMD or vector IR extensions remain a possible future direction.

9. **GPU-aligned numeric semantics** — Arithmetic is non-trapping where GLSL on typical GPUs is
   non-trapping; specifics are summarized below and detailed in the semantics chapter.

## Numeric semantics summary

LPIR numeric behavior follows GLSL-oriented, GPU-style rules: operations do not trap the abstract
machine for the cases listed below.

| Case                                         | Behavior                                      |
|----------------------------------------------|-----------------------------------------------|
| Float arithmetic                             | IEEE 754                                      |
| Integer arithmetic                           | Wrapping mod 2³²                              |
| Integer division / remainder by zero         | Result `0`                                    |
| Float division by zero                       | IEEE 754 (±Inf, NaN as defined by IEEE rules) |
| NaN in arithmetic                            | NaN propagates per IEEE rules                 |
| NaN in comparisons                           | Treated as false (`0` for condition values)   |
| Shift amount ≥ 32 bits                       | Shift amount masked to 5 bits                 |
| Float-to-integer conversion: overflow or NaN | Saturating to the representable integer range |

This table describes the **abstract** LPIR float interpretation. When targets run in **Q32**
fixed-point mode, float ops map to Q16.16 rules in [`../q32.md`](../q32.md) instead of IEEE 754 for
those paths.

Exact opcode mappings and edge cases are specified in the dedicated semantics documentation.

## Alternatives considered

### TempStack

A stack of temporaries can fix aliasing by construction but does not separate scalarization, builtin
lowering, and emission into a reusable middle-end. Structural coupling between expression walking
and code generation remains.

### Q32 as a shared LPIR→LPIR transform

A single IR rewrite for fixed-point would force one strategy on all backends. In practice,
WebAssembly and Cranelift benefit from different Q32 lowering tactics; keeping Q32 in emitters
avoids an ill-fitting common transform.

### Q32 in Naga → LPIR lowering

Making lowering mode-aware would duplicate IR shapes or branch the entire middle-end on numeric
mode, increasing complexity and test surface without a clear benefit over mode-aware emission.

### SPIR-V as the middle-end IR

SPIR-V is SSA-oriented, retains vector and structured types at the IR level, and carries a large
instruction set and toolchain surface. For this project’s goals (scalarized output, tight WASM
mapping, small owned IR), SPIR-V adds overhead and complexity without matching the desired shape.

### Full SSA with register allocation

A general SSA IR plus register allocation would duplicate work already done well inside Cranelift (
and is a poor fit for stack-oriented WASM emission). It is more machinery than needed for the
current pipeline.

## Crate layout

The LightPlayer GLSL stack is organized as follows:

```
lp-shader/
├── lpir/                    # LPIR core library (types, ops, builder, parser, printer, interpreter, validator)
├── lps-frontend/            # Naga → LPIR lowering (GLSL frontend)
├── lpvm-cranelift/          # LPIR → Cranelift → native machine code (RISC-V / host JIT)
└── lps-wasm/            # LPIR → WebAssembly emission (browser preview)
```

This layout reflects the separation of concerns: IR definition, frontend lowering, and per-target
emission. See [`lp-shader/CRATES.md`](../../lp-shader/CRATES.md) for the full crate index.
