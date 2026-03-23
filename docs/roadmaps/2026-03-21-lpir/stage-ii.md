# Stage II: lpir Crate Implementation

## Goal

Implement the `lpir` crate: Rust types for the IR, a builder API,
text format printer, text format parser, and an interpreter. Validate
with unit tests using hand-built IR.

## Suggested plan name

`lpir-stage-ii`

## Scope

**In scope:**
- `lp-glsl/lpir/` crate setup (Cargo.toml, no_std + alloc, no external deps)
- `Op` enum, `IrFunction`, `IrModule`, `IrType` (F32/I32), `VReg` type alias
- `VRegAllocator` — monotonic counter for fresh VRegs
- Builder API for constructing `IrFunction` (push ops, allocate VRegs)
- Text format printer (`IrFunction` → String)
- Text format parser (String → `IrFunction`)
- Interpreter: execute `IrFunction` with concrete inputs, return results
- Round-trip tests: build IR → print → parse → print → assert equal
- Interpreter tests: hand-built IR for known patterns (arithmetic, conditionals,
  loops, calls) → execute → verify results
- Validation: basic well-formedness checks (VReg used before defined, type
  mismatches)

**Out of scope:**
- Q32 expansion (lives in backend emitters, not in `lpir`)
- Naga lowering (Stage IV)
- WASM emission (Stage V)
- lpir-cli (deferred)
- Optimization passes

## Key decisions

- The crate must be `no_std` + `alloc`. No external dependencies.
- The builder API should make it easy to construct IR in the lowering
  (Stage IV). Pattern: `let dst = builder.alloc_vreg(IrType::F32);
  builder.push(Op::Fadd { dst, lhs, rhs });`
- The text printer should produce output identical to the spec examples.
- The parser should handle the full grammar from the spec.
- The interpreter operates on `Value` (`Float(f32)`, `Int(i32)`); conditions
  and comparison results are `Int(i32)` (`0` / `1` or nonzero).
  It executes float ops as `f32` (native float mode). Q32 is exercised only
  through backend emitters, not as distinct LPIR ops or interpreter modes.
- The `Op` enum is **LPIR-only** — no `i64` IR ops. Q32 widening lives inside
  WASM / Cranelift emission (`Stage V+`), not in `lpir`.

## Deliverables

- `lp-glsl/lpir/` crate with lib.rs, builder.rs, print.rs, parse.rs, interp.rs
- Unit tests covering all Op variants, control flow, round-trip printing,
  interpreter execution

## Dependencies

- Stage I (language specification) must be complete.

## Estimated scope

~900 lines of implementation + ~400 lines of tests.
