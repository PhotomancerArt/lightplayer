## Scope of Phase

Remove the obsolete per-node resolver cache from the generic runtime tree.

M2 moves same-frame resolution caching into the engine-owned resolver path, so
`NodeEntry` should no longer own `ResolverCache`. This phase is intentionally
mechanical: remove the field, update constructors/tests/docs that mention it,
and keep the existing resolver and `TickContext` tests compiling.

Out of scope:

- Do not implement `Engine`, `Resolver`, `ResolveSession`, or `BindingRegistry`.
- Do not rewrite `TickContext` yet; later phases route it through
  `ResolveSession`.
- Do not change `LegacyProjectRuntime`.

Suggested sub-agent model: `composer-2`.

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

Update these files:

- `lp-core/lpc-engine/src/tree/node_entry.rs`
- `lp-core/lpc-engine/src/tree/node_tree.rs`
- `lp-core/lpc-engine/src/tree/mod.rs` if module docs mention the old shape
- `lp-core/lpc-engine/README.md` if it still says `tree::NodeEntry` carries
  `ResolverCache`

Required changes:

1. Remove `use crate::resolver::ResolverCache;` from `node_entry.rs`.
2. Remove the `pub resolver_cache: ResolverCache` field from `NodeEntry<N>`.
3. Remove `resolver_cache: ResolverCache::new()` from `NodeEntry::new_spine`.
4. Update tests that assert a new entry/tree starts with an empty resolver cache.
   Delete those tests if they now only test the removed field.
5. Update any docs/comments that describe `NodeEntry` as owning resolved values.

Keep old resolver modules compiling. The standalone `ResolverCache` type may
remain for now because later phases will reshape it into the engine-owned query
cache.

## Validate

Run:

```bash
cargo test -p lpc-engine tree
cargo test -p lpc-engine resolver
```

If those commands expose nearby compile errors from stale imports, fix them
within this phase only when the fix is directly caused by removing
`NodeEntry::resolver_cache`.
