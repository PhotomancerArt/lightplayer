# Design: M9 Access L-values for `out` / `inout`

## Scope of Work

Add general support for passing writable Naga `Access` / `AccessIndex`
expressions to `out` and `inout` parameters.

The implementation should preserve existing bare-local `out` / `inout`
behavior and extend call lowering to supported writable access actuals:

- array elements;
- struct fields;
- nested struct fields;
- arrays-of-structs;
- vector lanes;
- matrix columns and cells where Naga emits writable access expressions;
- pointer-argument roots where the current function already has an addressable
  `out` / `inout` parameter buffer;
- private global roots where VMContext layout can provide an address or
  writeback path.

Uniform roots and uniform-derived paths remain read-only and must be rejected
clearly before a callee receives a writable pointer.

Out of scope:

- changing the aggregate pointer ABI;
- implementing unrelated `global-future/*` storage classes;
- patching/forking Naga for unrelated resolver issues;
- optimizing read-only `in` aggregate arguments beyond legality checks;
- fixing unrelated rv32 native call-frame failures;
- adding or validating deprecated `jit.q32` coverage.

## File Structure

```text
lp-shader/lps-frontend/src/
├── lib.rs                         # UPDATE: register new lowering module
├── lower_call.rs                  # UPDATE: use writable actual resolver for pointer args
├── lower_lvalue.rs                # NEW: shared writable access resolver + writeback model
├── lower_access.rs                # UPDATE: expose/reuse small address/write helpers if needed
├── lower_array.rs                 # UPDATE: expose aggregate element address helpers if needed
├── lower_struct.rs                # UPDATE: expose struct path address/writeback helpers if needed
└── lower_stmt.rs                  # UPDATE: keep Store behavior as source-of-truth for writeback

lp-shader/lps-filetests/filetests/function/
├── edge-lvalue-out.glsl           # UPDATE: retire markers proven fixed
└── access-lvalue-out-inout.glsl   # NEW: focused supported access actual coverage

lp-shader/lps-filetests/filetests/uniform/
└── write-error.glsl               # UPDATE or adjacent row: uniform actual rejected for out/inout

docs/roadmaps/2026-04-24-filetest-q32-cleanup/m9-access-lvalue-out-inout/
├── 00-notes.md
├── 00-design.md
└── NN-*.md                        # phase prompts
```

## Conceptual Architecture

```text
lower_user_call
  |
  | for callee TypeInner::Pointer actual
  v
resolve_writable_actual(actual_expr, pointee_ty)
  |
  +-- DirectAddress(addr)
  |     arrays / structs / aggregate subobjects with stable slot or VMContext address
  |
  +-- TempWriteback { addr, slot, writeback }
  |     scalar/vector/matrix locals, lanes, cells, and other non-addressable leaves
  |
  +-- Reject
        uniform roots, ParamReadOnly roots, non-lvalues, unsupported access shapes

call callee with addr
  |
  v
run queued writebacks after call
  |
  +-- bare local copyback
  +-- Store(Access...) style writeback via existing helpers
  +-- struct/array/global writeback via existing store helpers
```

## Main Components

### Writable Actual Resolver

`lower_lvalue.rs` owns the call-actual classification logic. Its central API
should take the current lowering context, the actual expression handle, and the
callee pointer pointee type, then return a call address plus any post-call
writeback work.

The resolver should distinguish:

- direct addresses for stable aggregate storage;
- temporary slots for scalar/vector/matrix values and leaves;
- explicit rejection for non-lvalues, uniforms, read-only aggregate params, and
  access shapes outside this milestone.

This keeps `lower_call.rs` focused on call ABI assembly rather than embedding
all writable expression forms inside call lowering.

### Direct Address Path

Direct addresses should be used when the destination is already represented by
stable storage compatible with the aggregate pointer ABI:

- bare local aggregates in `AggregateSlot::Local`;
- aggregate pointer arguments in `AggregateSlot::Param`;
- aggregate subobjects reachable from local aggregate slots;
- aggregate subobjects reachable from pointer-argument aggregate buffers;
- private global aggregate subobjects rooted in VMContext.

The direct path should compute `base + byte_offset` and pass that pointer to the
callee. It must only be used when the callee pointee type matches the resolved
subobject type well enough that the callee's loads/stores use the same layout.

### Temp Writeback Path

Temp writeback is the default for non-aggregate leaves and for access forms that
do not have a durable independent address in the current storage model:

- scalar locals;
- vector locals;
- matrix locals;
- vector lanes;
- matrix cells;
- scalar/vector/matrix struct fields when direct aggregate address passing is
  not appropriate;
- access leaves that existing `Store` lowering can write but cannot expose as a
  stable pointer.

The call lowering sequence is:

1. Load the current actual value for `inout` semantics where needed.
2. Allocate a temporary slot sized from the callee pointee IR types.
3. Store the initial value into the slot.
4. Pass the temporary slot address to the callee.
5. After the call, load the slot and write the result back to the actual.

The post-call writeback should reuse existing assignment/store helpers where
possible instead of duplicating every access mutation path in `lower_call.rs`.

### Uniform and Read-only Rejection

Uniform roots and uniform-derived access paths must not be passed to `out` or
`inout`. The resolver should reject these before the call, using a clear error
message consistent with existing write rejection such as
`"cannot write to uniform variable"`.

`AggregateSlot::ParamReadOnly` should also remain non-writable. If an access
path resolves through a read-only by-value `in` aggregate, the resolver should
reject rather than relying on debug assertions in lower-level store helpers.

### Tests

`edge-lvalue-out.glsl` remains the marker-retirement smoke test, but the main
acceptance surface should be a focused filetest for writable access actuals.

The focused function test should cover:

- bare local regression;
- array element `out` and `inout`;
- struct field `out` and `inout`;
- nested struct field;
- arrays-of-structs;
- vector lane;
- matrix column or cell, depending on the Naga expression shape;
- pointer-argument roots where a callee passes a writable subobject to another
  callee;
- private global writable access if implemented in the same phase group.

Uniform rejection should be tested in or near `uniform/write-error.glsl`.

Acceptance validation should target `wasm.q32`, `rv32c.q32`, and `rv32n.q32`.
Do not add new `jit.q32` annotations.
