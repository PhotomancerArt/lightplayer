# Notes: M9 Access L-values for `out` / `inout`

## Scope of Work

Milestone source:
`docs/roadmaps/2026-04-24-filetest-q32-cleanup/m9-access-lvalue-out-inout.md`

Add general support for passing writable Naga `Access` / `AccessIndex`
expressions to `out` and `inout` parameters. This should be the full Option B
design from the milestone, not a patch shaped only around current
`function/edge-lvalue-out.glsl` rows.

In scope:

- Preserve existing bare-local `out` / `inout` behavior.
- Accept writable local aggregate access actuals:
  - array elements;
  - struct fields;
  - nested struct fields;
  - arrays-of-structs;
  - vector lanes;
  - matrix columns / cells where Naga emits writable access expressions.
- Extend the same resolver to pointer-argument roots where a callee already
  has an addressable `out` / `inout` parameter buffer.
- Extend private global roots when the VMContext storage model can provide an
  address/writeback path.
- Reject uniform roots and uniform-derived access paths clearly before passing
  them to a callee as writable arguments.
- Retire `function/edge-lvalue-out.glsl` markers that are blocked only by
  access-shaped actual arguments.

Out of scope:

- Patching/forking Naga for unrelated resolver issues.
- Implementing `global-future/*` storage classes.
- Changing the aggregate pointer ABI.
- Optimizing read-only `in` aggregate arguments except where existing
  read-only handling affects legality checks.
- Fixing unrelated rv32 native call-frame traps such as the known
  `function/call-order.glsl` `rv32n.q32` failure.
- Reintroducing deprecated `jit.q32` validation.

## Current State

Call lowering currently has a narrow pointer-argument path:

- `lp-shader/lps-frontend/src/lower_call.rs` lowers user calls.
- For any callee argument whose Naga type is `TypeInner::Pointer`, it calls
  `call_arg_pointer_local()`.
- `call_arg_pointer_local()` accepts only `Expression::LocalVariable`; any
  `Access` / `AccessIndex` actual fails with
  `"inout/out call argument must be a local variable"`.
- Bare local scalar/vector/matrix values are copied into a temporary slot,
  passed by pointer, and copied back after the call.
- Bare local aggregates already pass their existing aggregate storage address.

Several existing lowering paths already know how to read/write the access
forms this milestone needs:

- `lp-shader/lps-frontend/src/lower_access.rs`
  - `lower_access_expr_vec()` reads dynamic `Access` roots.
  - `store_through_access()` writes dynamic array/vector/matrix/global access
    paths.
  - It already rejects writes to uniform globals with
    `"cannot write to uniform variable"`.
- `lp-shader/lps-frontend/src/lower_stmt.rs`
  - `Statement::Store` handles `Access` and `AccessIndex` pointer shapes.
  - It recognizes array-of-struct chains, struct local field paths, private
    global struct field paths, array element paths, vector lanes, matrix
    columns/cells, and pointer-argument member/component stores.
- `lp-shader/lps-frontend/src/lower_struct.rs`
  - `peel_struct_access_index_chain_to_local()` and
    `peel_struct_access_index_chain_to_global()` peel const member paths.
  - `store_struct_path_into_local()` and `store_struct_path_into_global()`
    write scalar/vector/matrix leaves.
  - `load_struct_path_from_local()` and `load_struct_path_from_global()` read
    struct paths.
- `lp-shader/lps-frontend/src/lower_array.rs`
  - `array_element_address()` can compute slot addresses for array elements.
  - Element load/store helpers already use clamped indexing semantics.
- `lp-shader/lps-frontend/src/lower_ctx.rs`
  - `AggregateSlot::{Local, Param, ParamReadOnly, Global}` captures existing
    storage roots.
  - `ParamReadOnly` must never be written.

Likely implementation shape:

- Add a shared writable-actual resolver, probably in a new
  `lp-shader/lps-frontend/src/lower_lvalue.rs` or similar module.
- The resolver should classify a call actual into either:
  - a direct address that can be passed to the callee; or
  - a temporary slot plus a post-call writeback operation.
- Reuse existing store helpers for writeback instead of duplicating all access
  mutation logic in `lower_call.rs`.
- Keep direct addresses for already-addressable aggregate slots and aggregate
  subobjects where the existing ABI layout makes that safe.
- Use temp/writeback for scalar/vector/matrix lanes and for access forms where
  a stable pointer to the exact destination is not already represented by the
  current storage model.

Current filetest surface:

- `lp-shader/lps-filetests/filetests/function/edge-lvalue-out.glsl` has
  `@broken` markers on bare local scalar/vector/int rows as well as access
  rows. The local rows are not the long-term access blocker and should be
  re-run before deciding which markers M9 removes.
- `lp-shader/lps-filetests/filetests/function/param-out-array.glsl` already
  covers whole-array `out` / `inout`; it does not currently pass an array
  element as the actual to a scalar `out` / `inout`.
- `lp-shader/lps-filetests/filetests/array/of-struct/out-param.glsl` and
  `inout-param.glsl` cover whole arrays of structs as parameters, not
  access-shaped actuals such as `ps[i].x`.
- Uniform tests exist under `lp-shader/lps-filetests/filetests/uniform/`, but
  a targeted rejected-uniform-write `out` / `inout` actual may need a new row.

Validation commands should stay focused during phases, then broaden:

- Targeted filetests through `scripts/glsl-filetests.sh --target wasm.q32`,
  `--target rv32c.q32`, and `--target rv32n.q32` for affected files/targets if
  the harness supports filtering.
- Otherwise run `scripts/glsl-filetests.sh --target <target>` for each q32
  target.
- For Rust-level validation: `cargo test -p lps-frontend` and/or
  `cargo check -p lps-frontend`.
- Final roadmap acceptance should include `just test-filetests`; if shader
  pipeline files are touched, also include the firmware validation required by
  workspace rules when practical:
  `cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server`.

## Questions

All initial questions were confirmed by the user with "all yes" on
2026-04-28.

### Q1. Should scalar/vector/matrix access actuals use temp-plus-writeback as the default?

Context: existing call lowering already uses a temporary slot plus copyback for
bare local non-aggregate `out` / `inout` actuals. Vector lanes and matrix cells
do not have durable independent stack addresses in the current vreg-backed local
model, while existing `Store(Access...)` helpers can write the post-call value
back correctly.

Suggested answer: Yes. Treat non-aggregate access leaves as temp/writeback by
default. Only pass direct addresses when the access resolves to an existing
aggregate byte region with a stable address.

Answer: Yes.

### Q2. Should aggregate subobjects prefer direct addresses when the current layout can compute one?

Context: arrays, structs, arrays-of-structs, and private globals already live in
slot/VMContext memory. For these, passing `base + offset` should avoid
unnecessary copies and align with the aggregate pointer ABI, as long as the
callee formal type is exactly the subobject type.

Suggested answer: Yes. Use direct address for aggregate subobjects with an
addressable storage root and known byte offset. Fall back to temp/writeback for
non-addressable or scalar/vector/matrix leaves.

Answer: Yes.

### Q3. Should private globals be included in the first implementation phase or deferred after local/pointer roots?

Context: M9 says private globals are in scope where VMContext layout can safely
provide an address/writeback path. Existing store helpers support private global
array and struct paths, but global handling broadens the resolver beyond local
slot and pointer-argument roots.

Suggested answer: Include private globals in the design, but phase them after
local and pointer-argument roots. That keeps the first implementation
reviewable while avoiding a design that has to be reworked for globals.

Answer: Yes.

### Q4. Should tests add new focused files or extend `edge-lvalue-out.glsl`?

Context: `edge-lvalue-out.glsl` currently mixes bare locals, commented invalid
examples, array element, swizzle, struct field, and stale-looking `@broken`
markers. The milestone also needs nested struct, arrays-of-structs, matrix
cell, pointer-argument, private global, and uniform-rejection coverage.

Suggested answer: Keep `edge-lvalue-out.glsl` as the marker-retirement smoke
test, but add one focused filetest for the new matrix of supported writable
actuals and one focused negative uniform test. This avoids turning the edge file
into the sole acceptance surface.

Answer: Yes.

### Q5. Should validation exclude `jit.q32` everywhere in this plan?

Context: the milestone explicitly says `jit.q32` is deprecated and not part of
acceptance. Some existing annotations still mention old target names such as
`@unimplemented(jit.q32)`.

Suggested answer: Yes. Do not add new `jit.q32` annotations or validation.
Only remove/update old ones if the touched file’s expectations require it.

Answer: Yes.
