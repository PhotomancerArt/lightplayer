# Phase 1: Shared resource identity

## Scope of phase

Move the small resource identity types into `lpc-model` so engine, wire, and
view code share one vocabulary for runtime buffers and render products.

In scope:

- Add a new `lp-core/lpc-model/src/resource.rs`.
- Move or re-create `RuntimeBufferId` and `RenderProductId` in `lpc-model`.
- Add shared `ResourceDomain` and `ResourceRef` types.
- Update `lpc-engine` to import/re-export these shared ids instead of defining
  independent local id types.
- Preserve the monotonic/no-reuse id invariant in store tests/docs.

Out of scope:

- Wire `GetChanges` resource fields.
- Resource summaries or payloads.
- Node-owned resource allocation.

## Code organization reminders

- Prefer one concept per file.
- Put public types and impls near the top, helper/test code at the bottom.
- Keep compatibility re-exports small and obvious.
- Do not add TODOs unless a real follow-up is unavoidable.

## Sub-agent reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or add `#[allow(...)]`.
- Do not disable, skip, or weaken tests.
- If blocked by an unexpected dependency cycle or public API issue, stop and
  report.
- Report files changed, validation commands, and deviations.

## Implementation details

Read:

- `docs/roadmaps/2026-05-01-runtime-core/m4.1-runtime-buffer-sync-projection/00-notes.md`
- `docs/roadmaps/2026-05-01-runtime-core/m4.1-runtime-buffer-sync-projection/00-design.md`
- `lp-core/lpc-engine/src/runtime_buffer/runtime_buffer_id.rs`
- `lp-core/lpc-engine/src/render_product/render_product_id.rs`
- `lp-core/lpc-model/src/lib.rs`

Add `lpc-model/src/resource.rs` with:

- `RuntimeBufferId(u32)`
- `RenderProductId(u32)`
- `ResourceDomain` with variants for runtime buffers and render products
- `ResourceRef` that carries `{ domain, id }` without generation

Keep the id APIs currently used by engine code:

- `new(raw: u32) -> Self`
- `as_u32(&self) -> u32`

Derives should match the current id types and be wire/view friendly:
`Copy`, `Clone`, `Debug`, `PartialEq`, `Eq`, `PartialOrd`, `Ord`, `Hash`,
`serde::Serialize`, `serde::Deserialize` where appropriate.

Update `lpc-model/src/lib.rs` to export the new types.

Update `lpc-engine`:

- Replace local id type definitions with re-exports from `lpc_model`.
- Existing paths like `crate::runtime_buffer::RuntimeBufferId` and
  `crate::render_product::RenderProductId` should continue to work if possible.
- Store allocation should remain monotonic and never reuse ids.

Add or update tests:

- id raw round-trip tests in `lpc-model`;
- engine store tests still compile and pass;
- a small `ResourceRef` test covering buffer and render refs.

## Validate

Run:

```bash
cargo test -p lpc-model resource
cargo test -p lpc-engine runtime_buffer
cargo test -p lpc-engine render_product
```
