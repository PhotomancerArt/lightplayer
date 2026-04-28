# Deferred: General Access L-values for `out` / `inout`

## Status

Deferred from the filetest q32 cleanup roadmap. Do not implement a
test-shaped subset in M3. This needs a separate milestone with a full
design.

Milestone file:
`docs/roadmaps/2026-04-24-filetest-q32-cleanup/m9-access-lvalue-out-inout.md`

## Problem

Current call lowering accepts only bare local variables for `out` and
`inout` pointer arguments. Naga represents many legal writable GLSL
l-values as `Access` / `AccessIndex` expressions instead:

```glsl
foo_out(arr[i]);
foo_out(s.field);
foo_out(s.inner.value);
foo_inout(points[i].x);
```

Supporting only the current filetest shapes would likely create a
partial path that has to be replaced later. The right fix is a general
resolver from writable Naga access expressions to an address/slot/write
primitive compatible with the aggregate pointer ABI.

## Design Notes For Future Milestone

- Map Naga expression shapes for every writable access form emitted by
  the current GLSL frontend.
- Distinguish local slots, aggregate slots, pointer arguments, globals,
  and uniforms.
- Uniform writes must remain rejected.
- The helper should answer: can this expression be passed by pointer to
  an `out` / `inout` callee, and if so what address or temporary
  writeback sequence is required?
- Arrays, structs, nested structs, arrays-of-structs, vector lanes, and
  matrix cells should be considered together.
- Global/uniform storage overlaps with the M5 memory model and should
  not be bolted on independently.

## Likely Acceptance Tests

- `function/edge-lvalue-out.glsl` (today: whole-file compile fails once an **access-shaped** `out`/`inout` actual appears, e.g. array element; local-variable forms are not the long-term blocker.)
- Any filetests involving `out` / `inout` to array elements, struct
  fields, nested fields, vector lanes, or matrix elements.
- Regression coverage that plain local `out` / `inout` behavior is
  unchanged.
