# Phase 2: Support Local Scalar/Vector/Matrix Access Writeback

## Scope of Phase

Extend the writable actual resolver to support local non-aggregate
`Access` / `AccessIndex` leaves by using temporary slots plus post-call
writeback.

In scope:

- Local vector lane actuals such as `scale(v.y)`.
- Local matrix column actuals where the callee pointee type is a vector.
- Local matrix cell actuals such as `scale(m[1][0])` when Naga emits supported
  `Access` / `AccessIndex` forms.
- Existing bare-local behavior from Phase 1 must remain unchanged.
- Uniform/global/pointer-argument roots remain out of scope for this phase.

Out of scope:

- Aggregate subobject direct addresses.
- Struct fields and arrays-of-structs.
- Pointer-argument roots.
- Private globals.
- New broad filetest marker cleanup; add only focused coverage needed for this
  phase.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests first.
- Place helper utility functions at the bottom of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

## Sub-agent Reminders

- Do not commit. The plan commits at the end as a single unit.
- Do not expand scope. Stay strictly within "Scope of Phase".
- Do not suppress warnings or `#[allow(...)]` problems away; fix them.
- Do not disable, skip, or weaken existing tests to make the build pass.
- If something blocks completion, stop and report back rather than
  improvising.
- Report back: what changed, what was validated, and any deviations from this
  phase plan.

## Implementation Details

Read first:

- `docs/roadmaps/2026-04-24-filetest-q32-cleanup/m9-access-lvalue-out-inout/00-design.md`
- `lp-shader/lps-frontend/src/lower_lvalue.rs`
- `lp-shader/lps-frontend/src/lower_access.rs`
- `lp-shader/lps-frontend/src/lower_stmt.rs`
- `lp-shader/lps-frontend/src/naga_util.rs`

Add resolver support for writable local scalar/vector/matrix leaves that do not
have durable independent addresses.

Suggested model:

- Add a writeback variant that stores the destination expression handle plus
  the pointee type and temp slot, for example:

```rust
WritableWriteback::AccessExpr {
    pointer: Handle<Expression>,
    pointee_ty: Handle<naga::Type>,
    slot: SlotId,
}
```

- When resolving a supported access leaf:
  - use `ctx.ensure_expr_vec(actual)` or existing access rvalue helpers to load
    the current value for `inout` semantics;
  - allocate a temp slot sized from `naga_type_to_ir_types(pointee_inner)`;
  - store the current value into the temp slot;
  - pass the temp slot address to the callee;
  - after the call, load values from the temp slot and write them back to the
    original access expression.

Prefer reusing existing store logic rather than duplicating all lane/cell
mutation code:

- If possible, add a helper that writes vregs from a temp slot back through a
  pointer expression by building on the same logic as `Statement::Store`.
- If direct reuse of `lower_access::store_through_access()` /
  `lower_stmt.rs` code is awkward because those APIs expect a value
  expression handle, add small lower-level helpers that accept already-loaded
  source vregs.
- Keep helpers focused and avoid moving unrelated `Statement::Store` logic.

Supported shapes should include:

- `Expression::AccessIndex { base: LocalVariable(vector), index }` for const
  vector lane access.
- `Expression::Access { base: LocalVariable(vector), index }` for dynamic
  vector lane access if the existing rvalue/store helpers support it.
- `Expression::AccessIndex { base: LocalVariable(matrix), index }` for const
  matrix column access.
- Nested matrix cell forms already supported by `lower_stmt.rs`, such as
  `AccessIndex(AccessIndex(LocalVariable(matrix), col), row)` and dynamic
  `Access(Access(LocalVariable(matrix), col), row)` if supported by existing
  lowerers.

Reject unsupported local access shapes with clear `UnsupportedExpression`
messages. Do not silently treat non-lvalues as writable.

Add focused filetest coverage only if needed to validate the phase locally. It
is acceptable for the broader test file to be completed in Phase 6, but this
phase should not rely solely on Rust compile checks.

## Validate

Run:

```bash
cargo check -p lps-frontend
cargo test -p lps-frontend
scripts/glsl-filetests.sh --target wasm.q32
```

If a narrower filetest invocation exists in the harness, also run the affected
function file(s) for `wasm.q32`. Report the exact command used.
