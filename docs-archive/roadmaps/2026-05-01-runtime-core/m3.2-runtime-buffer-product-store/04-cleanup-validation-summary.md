# Phase 4: Cleanup, Validation, and Summary

## Scope of Phase

Review the M3.2 changes, run final validation, and write the roadmap summary.

In scope:

- Check the M3.2 diff for scope creep, temporary code, debug prints, disabled
  tests, and warning suppressions.
- Run final validation.
- Write `summary.md` for the plan directory.

Out of scope:

- New feature work.
- Runtime node adapter work.
- Wire transport changes.
- Commits; the main agent handles commit decisions after review.

## Code Organization Reminders

- Keep cleanup edits minimal and directly tied to warnings/tests.
- Prefer fixing warnings over suppressing them.
- Keep `summary.md` terse and grep-friendly.
- Do not rewrite previous phase docs unless a factual correction is needed.

## Sub-agent Reminders

- Do not commit. The plan commits at the end as a single unit.
- Do not expand scope. Stay strictly within "Scope of phase".
- Do not suppress warnings or `#[allow(...)]` problems away; fix them.
- Do not disable, skip, or weaken existing tests to make the build pass.
- If validation fails with a non-trivial bug, stop and report rather than
  debugging deeply.
- Report back: files changed, validation run, result, cleanup findings, and any
  deviations.

## Implementation Details

Plan directory:

- `docs/roadmaps/2026-05-01-runtime-core/m3.2-runtime-buffer-product-store/`

Expected changed code:

- `lp-core/lpc-engine/src/runtime_buffer/*`
- `lp-core/lpc-engine/src/runtime_product/runtime_product.rs`
- `lp-core/lpc-engine/src/runtime_product/mod.rs`
- `lp-core/lpc-engine/src/engine/engine.rs`
- `lp-core/lpc-engine/src/lib.rs`
- maybe `lp-core/lpc-engine/src/resolver/production.rs`

Cleanup checks:

- Search the diff for:
  - `TODO`;
  - `todo!`;
  - `unimplemented!`;
  - `dbg!`;
  - `println!`;
  - `#[ignore]`;
  - new `#[allow(...)]`;
  - commented-out code.
- Existing TODOs outside touched lines are not automatically in scope.

Write `summary.md` with:

```markdown
### What was built

- ...

### Decisions for future reference

#### Runtime buffers are sibling store-backed products

- **Decision:** ...
- **Why:** ...
- **Rejected alternatives:** ...

#### Texture2D stays shader ABI, not RuntimeProduct::Value

- **Decision:** ...
- **Why:** ...
- **Rejected alternatives:** ...
```

Capture these decisions if they match the final implementation:

- `RuntimeBufferStore` is a sibling to `RenderProductStore`.
- `RuntimeBufferId` is generic; kind/metadata distinguish domains.
- Store entries use `Versioned<RuntimeBuffer>`.
- Legacy wire still receives full projected snapshots; refs/diffs/chunks are
  future work.
- `LpsValueF32::Texture2D` remains shader ABI but is rejected by checked runtime
  product value construction.

## Validate

Run:

```bash
cargo test -p lpc-engine
```
