# Phase 1: Add Writable Actual Resolver Skeleton

## Scope of Phase

Create the shared resolver module that call lowering will use for writable
`out` / `inout` actuals, but keep behavior equivalent to the current tree.

In scope:

- Add `lp-shader/lps-frontend/src/lower_lvalue.rs`.
- Register the module in `lp-shader/lps-frontend/src/lib.rs`.
- Move the existing bare-local pointer actual behavior out of
  `lower_call.rs` into the new resolver.
- Keep existing bare-local scalar/vector/matrix temp-slot copyback behavior.
- Keep existing bare-local aggregate direct-address behavior.
- Keep non-local `Access` / `AccessIndex` actuals rejected for now.

Out of scope:

- Supporting new access-shaped actuals.
- Adding new filetests beyond minimal Rust/unit coverage if needed.
- Changing aggregate pointer ABI or call argument order.
- Changing uniform/global behavior.

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
- `lp-shader/lps-frontend/src/lower_call.rs`
- `lp-shader/lps-frontend/src/lower_ctx.rs`
- `lp-shader/lps-frontend/src/lower_array.rs`

Create a new resolver module with a small, explicit API. A suggested shape:

```rust
pub(crate) struct WritableActual {
    pub(crate) addr: VReg,
    pub(crate) writeback: Option<WritableWriteback>,
}

pub(crate) enum WritableWriteback {
    LocalFlat {
        local: Handle<LocalVariable>,
        slot: SlotId,
    },
}

pub(crate) fn resolve_writable_actual(
    ctx: &mut LowerCtx<'_>,
    actual: Handle<Expression>,
    pointee_ty: Handle<naga::Type>,
) -> Result<WritableActual, LowerError> {
    // Phase 1: bare local only.
}

pub(crate) fn apply_writable_writeback(
    ctx: &mut LowerCtx<'_>,
    writeback: WritableWriteback,
) -> Result<(), LowerError> {
    // Phase 1: bare local flat copyback only.
}
```

Use the existing logic from `lower_call.rs` as the behavioral source:

- If `actual` is an `Expression::LocalVariable(lv)` and `lv` is in
  `ctx.aggregate_map`, return the aggregate storage base address and no
  writeback.
- Otherwise for a bare local scalar/vector/matrix:
  - resolve the local vregs;
  - allocate a slot of `naga_type_to_ir_types(pointee).len() * 4`;
  - store the current local vregs into the slot;
  - return the slot address plus a local-flat writeback.
- Preserve current assumptions and errors around pointer formals.
- For all non-local actuals, return the same style of
  `LowerError::UnsupportedExpression("inout/out call argument must be a local variable")`
  for now.

Update `lower_call.rs` so `lower_user_call()` delegates pointer formal handling
to `lower_lvalue::resolve_writable_actual()` and queues returned writebacks.
After the call, run `lower_lvalue::apply_writable_writeback()` for each queued
writeback.

The resulting diff should make later phases add new resolver arms without
further restructuring `lower_user_call()`.

## Validate

Run:

```bash
cargo check -p lps-frontend
cargo test -p lps-frontend
scripts/glsl-filetests.sh --target wasm.q32
```

If `scripts/glsl-filetests.sh --target wasm.q32` is too broad for the local
iteration, run it at least once before reporting completion and include any
known pre-existing failures in the report.
