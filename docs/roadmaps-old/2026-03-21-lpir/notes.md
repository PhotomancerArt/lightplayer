# Notes

## Scope

Define and implement LPIR (LightPlayer Intermediate Representation), a shared
middle-end IR between the Naga GLSL frontend and target backends (WASM,
Cranelift, potentially others). Then rewrite the WASM backend to consume LPIR
instead of emitting directly from Naga IR.

## Purpose

LPIR is the intermediary between the Naga frontend and target backends. It
provides a single place to handle LightPlayer-specific GLSL semantics: LPFX
builtin calls and vector scalarization. Each backend implements the mechanical
translation from LPIR to its target, including Q32 float mode handling (which
is backend-specific).

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
- **Scalarized**: No vector types. `vec3 a + vec3 b` is three `fadd` ops
  on separate VRegs. Scalarization happens during lowering (Naga → LPIR).
- **Non-SSA**: VRegs can be reassigned (needed for loop variables, mutable
  locals). This keeps the lowering simple and maps naturally to both WASM
  locals and Cranelift's `FunctionBuilder::def_var`/`use_var`.
- **Structured control flow**: `if`/`loop`/`break`/`continue`/`return`. No CFG,
  no basic blocks. Maps 1:1 to WASM structured control flow. Converting to
  Cranelift's CFG (structured → basic blocks) is the easy direction.
- **Float-mode-agnostic**: Expresses GLSL semantics (`fadd`, `fmul`),
  not implementation (f32.add vs Q32 i64-widen). Float mode is a backend
  concern, handled in each emitter.
- **Width-aware VReg types**: Each VReg has a concrete type (`f32`, `i32`).
  Boolean conditions use `i32` (WASM-style).
  Op names use short CLIF-style prefixes (`fadd`, `isub`, `fconst`) without
  embedded widths; the VReg type resolves any ambiguity. Backends map
  VReg type + FloatMode → concrete target type (e.g. f32 → wasm f32 or Q32 i32).

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
"add two floats" (`fadd`) and the backend decides whether that's `f32.add` or
Q32 fixed-point arithmetic. Adding new float modes only requires a new emission
path, not changes to the IR or lowering.

### Q32 in the emitter rationale

Q32 strategies are fundamentally backend-specific:

- WASM: inline i64 sequences (native i64 support)
- Cranelift saturating: builtin calls (`__lp_q32_add` etc.) because riscv32imac
  lacks i64 div and i64 mul requires libcalls
- Cranelift wrapping: all-i32 using `imul`+`smulhi`+shifts

A shared LPIR→LPIR transform would need to pick one representation. Either
it emits i64 ops (wrong for Cranelift) or builtin calls (suboptimal for WASM).
Keeping Q32 in the emitter means each backend uses its optimal strategy. The
shared part (which ops need Q32 treatment) is just a dispatch table — not
enough to justify an IR transform layer.

This means LPIR has no i64 type. The IR type universe is `f32` and `i32`
(GLSL `bool` lowers to `i32`). Future extensions may add 64-bit types and
vectors.

### Structured control flow rationale

WASM requires structured control flow. Converting structured → CFG (for
Cranelift) is easy and well-understood. The reverse (CFG → structured, the
"relooper" problem) is hard. Keeping LPIR structured avoids ever needing a
relooper.

## Components

1. **Rust types**: `Op` enum, `IrFunction`, `IrType` (F32/I32), `VReg`, builder API
2. **Text format definition**: Human-readable representation of LPIR
3. **Text format emitter**: IR → text (for debugging, logging, test snapshots)
4. **Text format parser**: text → IR (for testing the emitter/backends without
   Naga, for hand-written test cases, for snapshot testing)
5. **Interpreter**: Executes LPIR with concrete values (`f32`, `i32`). Tests
   lowering without any backend.
6. No binary format (not needed for our scale)
7. No Q32 transform — Q32 is handled per-backend in the emitter

## Crate structure

**Answer**: Two crates:

- `lp-shader/lpir` — core library. IR types, builder API, text printer, text
  parser. `no_std` compatible (printer/parser behind `alloc` feature).
  Zero external dependencies.
- `lp-shader/lpir-cli` — optional CLI. Reads LPIR text, validates, prints stats,
  maybe runs through a backend. Depends on `lpir` + `std`. Can be deferred.

`lps-frontend` depends on `lpir` for the lowering (Naga → LPIR).
`lps-wasm` depends on `lpir` for the emission (LPIR → WASM).
`lps-cranelift` (future) depends on `lpir` for emission (LPIR → CLIF).

## Questions and answers

### 1. Flat IR with virtual registers vs TempStack?

**Answer**: IR approach. Decoupling scalarization from emission is the right
structural fix.

### 2. What goes in the IR vs what stays as backend emission details?

**Answer**: The IR is float-mode-agnostic. `FloatAdd`, `FloatMul`, etc.
No Q32-specific ops. Backends handle mode conversion.

### 3. Type system?

**Answer**: Own `IrType { F32, I32 }`. Two types only — Naga's `Sint`, `Uint`,
and `Bool` all map to `I32` during lowering. Each VReg has an associated `IrType`.

### 4. Naga expression arena handling?

**Answer**: `Vec<Option<VReg>>` cache indexed by expression handle. Lowering
processes expressions on-demand. Statement::Emit ranges become no-ops.

### 5. Where does Q32 expansion happen?

**Answer**: In each backend's emitter. The lowering is mode-unaware and emits
abstract float ops (`fadd`, `fmul`). Each emitter receives `FloatMode` and
handles Q32 expansion using its optimal strategy (WASM: inline i64, Cranelift:
builtin calls or all-i32). Originally planned as a shared LPIR→LPIR transform,
but investigation of the Cranelift backend showed Q32 strategies are
fundamentally backend-specific — a shared transform would force one
representation that doesn't fit all backends.

### 6. LPFX out-pointer ABI?

**Answer**: `i32.store` and `i32.load` as IR ops.

### 7. Phase II interaction?

**Answer**: IR refactor first (scalar coverage). Phase II builds on top.

### 8. Can LPIR target both WASM and CLIF?

**Answer**: Yes. VRegs map to WASM locals (1:1) and CLIF Variables (via
def_var/use_var). Structured control flow maps to WASM if/loop/block and to
CLIF basic blocks + branches. Q32 is handled per-backend in the emitter.

### 9. Optimization on non-SSA LPIR?

**Answer**: Accepted tradeoff. Both target runtimes optimize. Simple passes
(constant folding, dead VReg elimination, liveness-based local reuse) are
possible later if needed. SSA-level optimization would happen in CLIF.
