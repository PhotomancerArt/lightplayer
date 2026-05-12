# Phase 3 — Extend Spine `NodeEntry` for Runtime State

sub-agent: yes
parallel: -

# Scope of phase

Extend the new generic spine tree path (`lpc-engine/src/tree/`) so
`NodeEntry<N>` can carry runtime-spine metadata:

- `SrcNodeConfig`
- `ArtifactRef`
- `ResolverCache`

Keep the current legacy `ProjectRuntime` and its legacy `NodeEntry`
unchanged.

Out of scope:

- Do not replace `ProjectRuntime.nodes`.
- Do not port legacy runtime nodes.
- Do not implement resolver behavior.
- Do not add `ProjectDomain`.

# Code organization reminders

- Keep `tree/node_entry.rs` focused on entry state and metadata.
- Tests stay at the bottom of `node_entry.rs` or in relevant tree modules.
- Avoid large helper abstractions unless they remove real complexity.
- Keep comments current; remove old “coming soon” comments once fields land.

# Sub-agent reminders

- Do not commit.
- Do not expand into legacy cutover.
- Do not suppress warnings.
- Do not weaken tests.
- If generic bounds become invasive, stop and report.
- Report files changed, validation commands/results, and deviations.

# Implementation details

Read `00-notes.md` and `00-design.md` first.

Update `lp-core/lpc-engine/src/tree/node_entry.rs`.

Current `NodeEntry<N>` already has:

- `id`
- `path`
- `parent`
- `child_kind`
- `children`
- `status`
- `state`
- frame counters

Add fields for the new spine:

```rust
pub config: lpc_source::SrcNodeConfig,
pub artifact: crate::artifact::ArtifactRef,
pub resolver_cache: crate::resolver::ResolverCache,
```

Because current tests construct entries without config/artifact, add an
additional constructor rather than breaking existing ergonomics blindly.
Recommended shape:

- Keep `NodeEntry::new(...)` if it is useful for tests, but have it create
  a minimal placeholder config/artifact only if that is clean.
- Prefer adding `NodeEntry::new_spine(...)` or updating `new` to accept:
  - `config: SrcNodeConfig`
  - `artifact: ArtifactRef`
  - frame

If updating `NodeTree::new` and `add_child` to require config/artifact would
make tests noisy, introduce helper constructors in tests. Do not use fake
global state.

Update `lp-core/lpc-engine/src/tree/node_tree.rs` as needed so adding a child
can supply config/artifact.

Important: `ArtifactRef` from phase 2 is a handle, not a borrowed payload.
It should be cheap to clone/copy if designed that way. If not, derive/impl
`Clone` where appropriate.

Add tests proving:

- new entries store `SrcNodeConfig`.
- new entries store `ArtifactRef`.
- each entry starts with an empty `ResolverCache`.
- mutating resolver cache on one entry does not affect another.
- existing frame counter behavior still works.

# Validate

Run:

```bash
cargo +nightly fmt
cargo check -p lpc-engine
cargo test -p lpc-engine tree::
```
