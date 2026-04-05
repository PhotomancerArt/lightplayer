# Stage IV: Naga → LPIR Lowering

## Goal

Implement the lowering pass that converts a Naga Module into LPIR functions.
Covers scalar expressions, control flow, calls, and LPFX. The lowering is
completely float-mode-unaware. Validates by interpreting the output LPIR
and by printing LPIR text from real GLSL inputs.

## Suggested plan name

`lpir-stage-iv`

## Scope

**In scope:**
- `lps-naga/src/lower.rs` — the Naga → LPIR lowering pass
- Expression lowering: literals, arguments, locals (load/store), binary ops,
  unary ops, comparisons, casts, select, zero values, constants
- Statement lowering: emit (no-op), block, if/else, loop, break, continue,
  return, store, call
- Expression caching: `Vec<Option<VReg>>` indexed by Handle<Expression>
- Parameter aliasing: Naga's `in` parameters (LocalVariable + Store from
  FunctionArgument) mapped to parameter VRegs
- User function calls: `Statement::Call` → `Op::Call`
- LPFX calls: detect LPFX builtins, generate memory ops for out-params
- Math builtins that the current scalar backend handles: abs, round, min,
  max, mix, smoothstep, step, mod (decomposed into scalar LPIR ops)
- Tests: GLSL → Naga → LPIR → interpret, verify results
- Tests: GLSL → Naga → LPIR → print text, verify output

**Out of scope:**
- Vector expressions (Phase II follow-on)
- Vector builtins (Phase II follow-on)
- WASM emission (Stage V)

## Key decisions

- The lowering is completely float-mode-unaware. It emits `fadd`,
  `fconst.f32 1.5`, etc. Q32 handling is done inside each backend's
  emitter (Stage V), not as an IR transform.
- Expression caching must handle the DAG nature of Naga's arena — an
  expression referenced multiple times produces one set of VRegs.
- `Statement::Emit` ranges are no-ops; expressions are lowered on-demand
  when referenced as operands.
- Builtin decomposition (smoothstep, mix, etc.) happens here, producing
  sequences of scalar LPIR ops.
- LPFX handling in the lowering is limited to call structure and out-pointer
  ABI (generating `call` + `i32.store`/`i32.load` ops). How those calls are
  resolved (import names, signatures) is the emitter's job.
- The lowering can be tested independently of any backend by using the
  interpreter (Stage II) to execute the resulting LPIR.

## Deliverables

- `lps-naga/src/lower.rs` — complete scalar lowering
- Tests: known GLSL snippets → expected LPIR text output
- Tests: known GLSL snippets → LPIR → interpret → verify results

## Dependencies

- Stage II (lpir crate with interpreter) must be complete.
- Stage III (interpreter + validation hardening) should be complete so the
  interpreter is thoroughly tested before lowering builds on it.
- Existing `lps-naga` crate (compile, LPFX injection) is the base.

## Estimated scope

~800–1200 lines of lowering + ~200 lines of tests.
