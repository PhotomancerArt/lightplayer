# Stage I: LPIR Language Specification

## Goal

Define the complete LPIR language: operation set, type rules, text format
grammar, and semantics. Produce a specification document that is the reference
for all subsequent implementation.

## Suggested plan name

`lpir-stage-i`

## Scope

**In scope:**
- Complete Op set enumeration (float ops, int ops, bool ops, casts,
  comparisons, select, copy, control flow, calls, memory)
- ScalarKind type system (Float, Sint, Uint, Bool)
- VReg semantics (definition, reassignment, typing rules)
- Text format grammar (formal or semi-formal)
- Text format examples covering all ops and control flow patterns
- Semantics of each operation (what it means, not how it's implemented)
- Mapping table: GLSL operation → LPIR op(s)

**Out of scope:**
- Rust implementation (Stage II)
- Q32 transform (Stage III)
- Naga lowering (Stage IV)
- WASM emission (Stage V)
- Vector operations (future — Phase II of Naga migration)
- Optimization passes

## Key decisions

- The Op set should cover everything needed for scalar filetests today,
  plus the control flow and call patterns needed for Phase II later.
  Design for the full scope even though scalar-only is implemented first.
- The text format should be line-oriented and LL(1)-parseable for simplicity.
- Every Op needs clear documentation of its operand types and result type.

## Deliverables

- `docs/plans/<date>-lpir-stage-i/spec.md` — the LPIR specification document
  containing the full Op set, type rules, text format grammar, and examples.

## Dependencies

None — this is the first stage.

## Estimated scope

~1 document, primarily design work. The spec should be thorough enough that
Stage II implementation is mechanical.
