## Scope of Phase

Update the new-spine `TickContext` so nodes resolve values through
`ResolveSession`/`QueryKey` rather than the old borrowed per-node
`ResolverCache` and `ResolverContext` facade.

This phase connects node authoring ergonomics to the new resolver model while
keeping the API small and synchronous.

Out of scope:

- Do not implement new engine behavior beyond what is needed to compile with
  the `Engine` and `ResolveSession` APIs from prior phases.
- Do not port legacy `RenderContext`.
- Do not add render products or async resolution.

Suggested sub-agent model: `kimi-k2.5`.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place public items and entry points near the top, helpers below them, and
  `#[cfg(test)] mod tests` at the bottom of Rust files.
- Keep related functionality grouped together.
- Any temporary code must have a TODO comment so it can be found later.

## Sub-agent Reminders

- Do not commit. The plan commits at the end as a single unit.
- Do not expand scope. Stay strictly within "Scope of Phase".
- Do not suppress warnings or `#[allow(...)]` problems away; fix them.
- Do not disable, skip, or weaken existing tests to make the build pass.
- If something blocks completion, stop and report back rather than improvising.
- Report back what changed, what was validated, and any deviations from this
  phase plan.

## Implementation Details

Update:

- `lp-core/lpc-engine/src/node/contexts.rs`
- `lp-core/lpc-engine/src/node/node.rs` tests if needed
- `lp-core/lpc-engine/src/node/mod.rs` exports only if needed

Current `TickContext` takes:

- `SrcNodeConfig`
- `&mut ResolverCache`
- `ArtifactId`
- `artifact_content_frame`
- `&Bus`
- `&dyn ResolverContext`

That shape belongs to the old resolver. Replace or add a new constructor so the
new tick path is based on:

- current `NodeId`
- current `FrameId`
- a mutable resolver session or a narrow session facade
- artifact id/content frame if still needed for node-private cache checks

Target node-facing API:

```rust
impl TickContext<'_> {
    pub fn resolve(&mut self, query: QueryKey) -> Result<ProducedValue, ResolveError>;
    pub fn frame_id(&self) -> FrameId;
    pub fn node_id(&self) -> NodeId;
}
```

If returning owned `ProducedValue` is simpler and avoids lifetime issues, do
that. The M2 design intentionally uses `Versioned<LpsValueF32>` directly.

Keep or adapt convenience helpers only if they remain meaningful:

- `artifact_ref`
- `artifact_content_frame`
- `artifact_changed_since`

Remove old `bus_read`, `bus_last_writer_frame`, and `changed_since` helpers if
they only expose the old bus/cache model. If removing them breaks many existing
tests, keep them only where still backed by new resolver/session behavior.

Tests to update/add:

- a dummy node can call `ctx.resolve(QueryKey::Bus(...))`
- a dummy node can call `ctx.resolve(QueryKey::NodeInput { .. })`
- `TickContext` accessors still expose node/frame identity
- old tests no longer construct a fake `ResolverContext` just to satisfy
  `TickContext`

## Validate

Run:

```bash
cargo test -p lpc-engine node
cargo test -p lpc-engine resolver
cargo test -p lpc-engine engine
```
