# M3 Frontend Overloads and L-values — Notes

## Goal

Fix resolver, parser, overload, and local aggregate l-value failures in
the frontend.

## Current Findings

- `bvecN` `mix` appears to require both resolver and lowering work:
  Naga currently reports ambiguous best function for boolean-vector
  `mix`, and `lps-frontend` lowering is float-lerp oriented rather than
  boolean-select oriented.
- `const in` parameter syntax is likely in the GLSL parser / Naga
  grammar surface, not just `lps-frontend` lowering.
- Out/inout to non-local l-values is blocked by call lowering that only
  accepts `Expression::LocalVariable` for pointer arguments. Access
  l-values such as array elements and struct fields need a slot/pointer
  path. **Decision:** defer this to a later dedicated milestone and do
  the general Option B design properly rather than doing test-shaped
  support in this roadmap.
- `function/edge-return-type-match.glsl` and aggregate return/array
  assignment failures likely share sret/memcpy/slot wiring with the
  aggregate roadmap.
- `function/overload-same-name.glsl` is call/ABI/lowering behavior, not
  a backend-only issue.
- `function/call-order.glsl` overlaps with globals because the failing
  case mutates `global_counter` through side-effecting calls while
  evaluating arguments.
- `function/declare-prototype.glsl` vector `// run` parsing belongs to
  M2, not M3.

## Questions For User

- For `bvecN` `mix`, are Naga patches/forks acceptable, or should the
  implementation prefer a narrow workaround in our frontend pipeline?
  **Answered:** If Naga is broken on fringe builtin-resolution cases
  like this, mark the affected tests unsupported with a note rather than
  patching/forking Naga for this roadmap.
- For `const in`, is the target full GLSL 4.x qualifier-order support
  or only the `const in T` shape in the current filetests?
- For out/inout to `Access` l-values, should M3 support only the shapes
  present in the tests first (array elements / struct fields), or design
  full parity with all Naga access l-values? **Answered:** Defer from
  this roadmap. This deserves its own later milestone with the full
  Option B design; do not implement test-shaped support now.
- For `function/call-order.glsl`, should rv32-emu be the primary
  target, or must wasm and rv32 be fixed in the same pass?

## Implementation Notes

- Distinguish local frontend/l-value bugs from global/uniform memory
  bugs that belong to M5.
- Re-check aggregate ternary after frontend aggregate fixes.
- Map Naga expression shapes for out/inout arguments before changing
  lowering.
- Add boolean-vector `mix` as boolean select, not q32 lerp.
- If boolean-vector `mix` is blocked by Naga builtin-resolution
  ambiguity rather than our lowering, prefer `@unsupported` plus an
  explanatory note over a Naga patch/fork.
- For access-lvalue out/inout rows, keep them marked as known broken /
  deferred and move design notes to a later milestone rather than
  expanding M3 scope.
- If a row is really global store or vmctx state, leave it for M5.

## Validation

- Targeted frontend/overload filetests.
- Key files:
  `vec/bvec2/fn-mix.glsl`, `vec/bvec3/fn-mix.glsl`,
  `vec/bvec4/fn-mix.glsl`, `function/edge-const-out-error.glsl`,
  `function/edge-lvalue-out.glsl`,
  `function/overload-same-name.glsl`, and
  `function/call-order.glsl`.
- Final `just test-filetests`.
