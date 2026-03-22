# Notes

## Scope

Define and implement LPIR (LightPlayer Intermediate Representation), a shared
middle-end IR between the Naga GLSL frontend and target backends (WASM,
Cranelift, potentially others). Then rewrite the WASM backend to consume LPIR
instead of emitting directly from Naga IR.

## Purpose

LPIR is the intermediary between the Naga frontend and target backends. It
provides a single place to handle LightPlayer-specific GLSL semantics: LPFX
builtin calls, Q32 fixed-point mode awareness, and vector scalarization.
Each backend only needs to implement the mechanical translation from LPIR to
its target.

## Problem statement

The current WASM backend couples three concerns in a single pass: vector
scalarization, temporary local allocation, and WASM byte emission. This causes
scratch local aliasing bugs for nested expressions. We hit the same wall with
both the old custom frontend and the Naga frontend.

Separately, we have two backends (WASM, Cranelift) that currently share no
middle-end code. Each independently handles scalarization, builtins, and
GLSL→target translation. LPIR unifies this.

## IR classification

LPIR is a **flat, scalarized, non-SSA IR with structured control flow and
virtual registers**.

- **Flat / ANF-style**: Every intermediate value is bound to a named virtual
  register (VReg). No expression trees. Operations reference VRegs, not
  sub-expressions.
- **Scalarized**: No vector types. `vec3 a + vec3 b` is three `float.add` ops
  on separate VRegs. Scalarization happens during lowering (Naga → LPIR).
- **Non-SSA**: VRegs can be reassigned (needed for loop variables, mutable
  locals). This keeps the lowering simple and maps naturally to both WASM
  locals and Cranelift's `FunctionBuilder::def_var`/`use_var`.
- **Structured control flow**: `if`/`loop`/`break`/`continue`/`return`. No CFG,
  no basic blocks. Maps 1:1 to WASM structured control flow. Converting to
  Cranelift's CFG (structured → basic blocks) is the easy direction.
- **Float-mode-agnostic**: Expresses GLSL semantics (`float.add`, `float.mul`),
  not implementation (`f32.add` vs Q32 i64-widen). Float mode is a backend
  concern.
- **Typed**: Each VReg has a `ScalarKind` (Float, Sint, Uint, Bool). Backends
  map ScalarKind + FloatMode → concrete type (e.g. Float → f32 or i32).

Prior art: QBE (flat ops + virtual registers), Binaryen flat form (structured
control flow + named intermediates), A-Normal Form (every intermediate named).

## Design decisions

### Non-SSA rationale

Making LPIR SSA would complicate the lowering (explicit merge/phi nodes for
loops and if-branches) and the WASM emitter (de-SSA to convert back to mutable
locals). Both target runtimes (WASM JIT engines, Cranelift) perform their own
SSA construction and optimization. The benefit of LPIR-level SSA optimization
doesn't justify the complexity.

Simple optimizations (constant folding, dead VReg elimination, copy propagation,
liveness-based local reuse) are still possible on non-SSA LPIR if needed later.

### Float-mode-agnostic rationale

Mirrors the Cranelift backend's `NumericMode` dispatch pattern. The IR expresses
"add two floats" and the backend decides whether that's `f32.add` or Q32
fixed-point arithmetic. Adding new float modes (F16, BFloat16, different
fixed-point) only requires a new emission path, not changes to the IR or
lowering.

### Structured control flow rationale

WASM requires structured control flow. Converting structured → CFG (for
Cranelift) is easy and well-understood. The reverse (CFG → structured, the
"relooper" problem) is hard. Keeping LPIR structured avoids ever needing a
relooper.

## Components

1. **Rust types**: `Op` enum, `IrFunction`, `ScalarKind`, `VReg`, builder API
2. **Text format definition**: Human-readable representation of LPIR
3. **Text format emitter**: IR → text (for debugging, logging, test snapshots)
4. **Text format parser**: text → IR (for testing the emitter/backends without
   Naga, for hand-written test cases, for snapshot testing)
5. **Interpreter**: Executes LPIR with concrete values (f32, i32, bool). Tests
   lowering and Q32 transform without any backend.
6. **Q32 transform**: LPIR → LPIR consuming pass that rewrites float ops to
   i32/i64 fixed-point sequences. Shared by all backends.
7. No binary format (not needed for our scale)

## Crate structure

**Answer**: Two crates:
- `lp-glsl/lpir` — core library. IR types, builder API, text printer, text
  parser. `no_std` compatible (printer/parser behind `alloc` feature).
  Zero external dependencies.
- `lp-glsl/lpir-cli` — optional CLI. Reads LPIR text, validates, prints stats,
  maybe runs through a backend. Depends on `lpir` + `std`. Can be deferred.

`lp-glsl-naga` depends on `lpir` for the lowering (Naga → LPIR).
`lp-glsl-wasm` depends on `lpir` for the emission (LPIR → WASM).
`lp-glsl-cranelift` (future) depends on `lpir` for emission (LPIR → CLIF).

## Questions and answers

### 1. Flat IR with virtual registers vs TempStack?

**Answer**: IR approach. Decoupling scalarization from emission is the right
structural fix.

### 2. What goes in the IR vs what stays as backend emission details?

**Answer**: The IR is float-mode-agnostic. `FloatAdd`, `FloatMul`, etc.
No Q32-specific ops. Backends handle mode conversion.

### 3. Type system?

**Answer**: Own `ScalarKind { Sint, Uint, Float, Bool }` (same names as Naga,
no dependency). Each VReg has an associated ScalarKind.

### 4. Naga expression arena handling?

**Answer**: `Vec<Option<VReg>>` cache indexed by expression handle. Lowering
processes expressions on-demand. Statement::Emit ranges become no-ops.

### 5. Where does Q32 expansion happen?

**Answer**: LPIR → LPIR consuming transform pass (Stage III). The lowering
(Stage IV) is mode-unaware and emits `float.*` ops. In Q32 mode, the transform
rewrites them to `i32.*`/`i64.*` sequences. This keeps the lowering simple,
shares Q32 logic across backends, and keeps emitters mechanical. The transform
uses Rust move semantics to consume the input `Vec<Op>` and produce a new one,
minimizing peak memory.

### 6. LPFX out-pointer ABI?

**Answer**: `i32.store` and `i32.load` as IR ops.

### 7. Phase II interaction?

**Answer**: IR refactor first (scalar coverage). Phase II builds on top.

### 8. Can LPIR target both WASM and CLIF?

**Answer**: Yes. VRegs map to WASM locals (1:1) and CLIF Variables (via
def_var/use_var). Structured control flow maps to WASM if/loop/block and to
CLIF basic blocks + branches. Q32 is handled by the shared transform pass
(Stage III) before reaching either backend.

### 9. Optimization on non-SSA LPIR?

**Answer**: Accepted tradeoff. Both target runtimes optimize. Simple passes
(constant folding, dead VReg elimination, liveness-based local reuse) are
possible later if needed. SSA-level optimization would happen in CLIF.
