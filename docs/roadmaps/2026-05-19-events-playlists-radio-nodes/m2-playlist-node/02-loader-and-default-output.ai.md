# Phase 2: Recursive Loader And Default Visual Output Binding

- parallel: -
- sub-agent: supervised

## Scope Of Phase

Refactor project loading so node invocations can be loaded recursively, then add default visual
output binding policy with suppression for structurally owned playlist entry children.

In scope:

- recursive `NodeInvocation` loading helper;
- ownership/default-output policy metadata;
- entry-local binding registration for nested playlist entry slots;
- nested relative node reference resolution;
- default `output -> bus#visual.out` binding for top-level visual nodes;
- tests proving playlist entry children do not leak default visual output.

Out of scope:

- `PlaylistNode` runtime behavior.
- Crossfade rendering.
- Example artwork.

## Code Organization Reminders

- Keep loader helpers small and named by responsibility.
- Prefer a small ownership enum over booleans passed through long call chains.
- Avoid rewriting project loading beyond what recursive child loading needs.
- Tests stay at the bottom of `project_loader.rs` or in existing nearby test modules.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Primary file:

```text
lp-core/lpc-engine/src/engine/project_loader.rs
```

Likely supporting files:

```text
lp-core/lpc-engine/src/node/node_tree.rs
lp-core/lpc-engine/src/node/tree_error.rs
```

Refactor current root-only loading into a helper along these lines:

```text
load_child_invocation(
    root,
    runtime,
    loaded_nodes,
    parent_id,
    child_name,
    invocation,
    source_base_path,
    ownership,
    frame,
)
```

Add an ownership/policy type such as:

```rust
enum LoadedNodeOwnership {
    ProjectChild,
    PlaylistEntry { playlist: NodeId, entry: u32 },
}
```

Store enough metadata on `LoadedNode` to support nested references and default-output decisions:

- `parent: Option<NodeId>`
- `ownership`
- `suppress_visual_default_output: bool`

Recursive loading rules:

- `ProjectDef.nodes` calls the helper with `ProjectChild`.
- `PlaylistDef.entries` calls the helper for each `entry.node` with `PlaylistEntry`.
- Use `entry.name` when present and valid.
- Otherwise derive `entry_<index>`.
- Reject duplicate child names under the same parent.
- Path-backed children resolve relative to the containing node definition file.
- Inline children use the containing file as `source_base_path`.

Entry-local bindings:

- `PlaylistEntry` owns a `bindings: BindingDefs` field.
- Register source bindings from each entry's binding map against the owning playlist node, not the
  child shader node.
- The first required entry-local binding is `trigger`; `[entries.2.bindings.trigger]` becomes a
  consumed binding targeting slot path `entries[2].trigger` on the playlist node.
- Keep the helper generic enough to add future entry-local consumed slots without making authors
  write quoted top-level binding keys such as `[bindings."entries[2].trigger"]`.

Relative node refs:

- Expand `resolve_relative_node_ref` so `..` resolves to parent.
- `..sibling` should still work.
- `.child` resolves to a child of current.
- `..#entry_time` must resolve from an entry child shader to the owning playlist.
- Keep unsupported deeper forms explicit with clear errors if full generality is too much for this
  phase, but cover parent and sibling/child cases used by the example.

Default visual output binding:

- Add `register_visual_default_output_binding`.
- Apply it to `Shader`, `Fluid`, and `Playlist` once playlist runtime lands.
- In this phase, implement and test for existing `Shader` and `Fluid`.
- It should add:

```text
source: ProducedSlot { node, slot: "output" }
target: BusChannel("visual.out")
priority: BindingPriority::default_fallback()
```

- Do not add it when:
  - the node has an explicit `output` target binding;
  - `suppress_visual_default_output` is true.
- Explicit child output bindings must still be registered if authored.

Tests:

- A top-level shader with no output binding can satisfy fixture input from `bus#visual.out`.
- A shader with explicit output target does not receive an extra default output binding.
- Two top-level visual nodes with no explicit binding produce an ambiguous fallback bus provider;
  this preserves current bus semantics and tells authors to bind explicitly.
- Playlist entry child shader with no explicit output target does not register a provider for
  `visual.out`.
- Entry-local trigger binding registers a consumed binding for `entries[2].trigger` on the playlist
  node.
- A child shader can bind `time` from `..#entry_time` once the playlist node exists; if this test
  cannot be fully wired until Phase 4, add a loader-level relative-ref test with a dummy parent
  produced slot.

## Validate

Run:

```bash
cargo test -p lpc-engine project_loader
cargo test -p lpc-engine node_tree
cargo check -p lpc-engine
```
