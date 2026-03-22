# Stage V: LPIR → WASM Emission

## Goal

Rewrite the WASM emitter to consume LPIR. The emitter is purely mechanical:
each Op maps to one or a small number of WASM instructions. Local allocation
is trivial (VReg N → local N). Validate with wasmtime smoke tests.

## Suggested plan name

`lpir-stage-v`

## Scope

**In scope:**
- Rewrite `lp-glsl-wasm/src/emit.rs` to walk `IrFunction.body` (Vec<Op>)
- Local declaration: count VRegs by type, declare as function locals
  (f32 locals for Float VRegs, i32 for Sint/Uint/Bool, i64 for I64 VRegs)
- Op emission: 1:1 map for most ops
  - `float.add` → `local.get lhs`, `local.get rhs`, `f32.add`, `local.set dst`
  - `i32.add` → `local.get lhs`, `local.get rhs`, `i32.add`, `local.set dst`
  - Control flow: `if` → `block/br_if/block`, `loop` → `loop/br_if/br`
  - `call` → WASM call instruction
  - `i32.store`/`i32.load` → WASM memory ops
  - `return` → WASM return
- Function parameters: first N locals are params (WASM convention)
- LPFX import handling: detect `call @lpfx_*` ops, generate WASM imports
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
- In Q32 mode, the LPIR has already been transformed (Stage III), so the
  emitter only sees `i32.*` and `i64.*` ops for numeric work. No mode
  dispatch in the emitter.
- In float mode, the LPIR contains `float.*` ops, and the emitter maps them
  to `f32.*` WASM instructions.
- The emitter is simple enough that bugs should be rare. Most testing effort
  goes into the lowering (Stage IV) and transform (Stage III) via the
  interpreter.

## Deliverables

- Rewritten `lp-glsl-wasm/src/emit.rs` (~300 lines)
- Deleted `lp-glsl-wasm/src/locals.rs`, `emit_vec.rs`
- Simplified `lp-glsl-wasm/src/types.rs`
- Wasmtime-based tests exercising the full pipeline
  (GLSL → Naga → LPIR [→ Q32 transform] → WASM → run)

## Dependencies

- Stage IV (Naga → LPIR lowering) must produce valid IR.
- Stage III (Q32 transform) must be complete for Q32-mode emission.

## Estimated scope

~300 lines of emitter + ~100 lines of tests.
