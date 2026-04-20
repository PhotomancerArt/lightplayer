# Stage V: LPIR → WASM Emission

## Goal

Rewrite the WASM emitter to consume LPIR. The emitter is purely mechanical:
each Op maps to one or a small number of WASM instructions. Local allocation
is trivial (VReg N → local N). Validate with wasmtime smoke tests.

## Suggested plan name

`lpir-stage-v`

## Scope

**In scope:**
- Rewrite `lps-wasm/src/emit.rs` to walk `IrFunction.body` (Vec<Op>)
- Local declaration: count VRegs by type, declare as function locals
  (f32 locals for Float VRegs, i32 for Int VRegs)
- Op emission: 1:1 map for most ops
  - `float.add` → `local.get lhs`, `local.get rhs`, `f32.add`, `local.set dst`
  - `i32.add` → `local.get lhs`, `local.get rhs`, `i32.add`, `local.set dst`
  - Control flow: `if` → `block/br_if/block`, `loop` → `loop/br_if/br`
  - `call` → WASM call instruction
  - `load`/`store` → WASM memory ops on linear memory (shadow stack frame)
  - `return` → WASM return
- Function parameters: first N locals are params (WASM convention)
- Shadow stack for `slot`/`slot_addr`: mutable `$sp` global, per-function
  prologue/epilogue for functions with slot declarations, elided when no
  slots. LPFX scratch becomes ordinary slots — no global scratch region.
- LPFX import handling: detect `call @lpfn_*` ops, generate WASM imports
- Deletion of `locals.rs` and `emit_vec.rs`
- Simplification of `types.rs`
- Wasmtime smoke tests: compile LPIR → WASM → run → verify

**Out of scope:**
- Optimization (dead local elimination, peephole) — future work
- Vector support (not needed, LPIR is scalarized)
- Cranelift backend (future stage)

## Key decisions

- VReg index = WASM local index (offset by parameter count for function-local
  VRegs). This makes local allocation trivial.
- The emitter is **mode-aware**: LPIR contains float-agnostic ops (`fadd`,
  `fmul`); the WASM emitter maps them to `f32.*` instructions in float
  mode, or expands them to inline i64 Q32 sequences in Q32 mode. Q32
  expansion is emitter-internal — no separate IR transform.
- The emitter is simple enough that bugs should be rare. Most testing effort
  goes into the lowering (Stage IV) via the interpreter.

## Deliverables

- Rewritten `lps-wasm/src/emit.rs` (~300 lines)
- Deleted `lps-wasm/src/locals.rs`, `emit_vec.rs`
- Simplified `lps-wasm/src/types.rs`
- Wasmtime-based tests exercising the full pipeline
  (GLSL → Naga → LPIR → WASM emitter [Q32 inside] → run)

## Dependencies

- Stage IV (Naga → LPIR lowering) must produce valid IR.

## Estimated scope

~300 lines of emitter + ~100 lines of tests.
