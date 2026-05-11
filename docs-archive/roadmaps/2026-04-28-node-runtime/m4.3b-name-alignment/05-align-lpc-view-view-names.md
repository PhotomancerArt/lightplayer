# Phase 5 — Align `lpc-view` View Names

sub-agent: yes
parallel: -

# Scope of phase

Rename stale client/wire-style view/cache names in `lpc-view` to natural
`*View` suffix names:

- `ClientProjectView` -> `ProjectView`
- `ClientNodeEntry` -> `NodeEntryView`
- `ClientNodeTree` -> `NodeTreeView`
- `ClientTreeEntry` -> `TreeEntryView`
- `WirePropAccess` -> `PropAccessView`
- `WirePropsMap` -> `PropsMapView`
- `StatusChange` -> `StatusChangeView`

Inspect `ClientApi`; rename it only if it is clearly a view-owned
abstraction rather than a real client/app API.

Out of scope:

- Changing tree-apply behavior.
- Changing wire message processing.
- Renaming app client/server crates.

# Code organization reminders

- Prefer one concept per file.
- File names should follow primary public type names:
  `project_view.rs`, `node_tree_view.rs`, `tree_entry_view.rs`,
  `prop_access_view.rs` if practical.
- Keep tests at the bottom of their modules.
- Do not keep compatibility aliases for old view names.

# Sub-agent reminders

- Do not commit.
- Stay within `lpc-view` naming and required call-site updates.
- Do not suppress warnings or weaken tests.
- If `ClientApi` ownership is ambiguous, leave it unchanged and report
  the reasoning.
- Report changed files, validation commands/results, and deviations.

# Implementation details

Read `00-notes.md` and `00-design.md` in this directory first.

In `lp-core/lpc-view/src/`:

- Rename `project/view.rs` to `project/project_view.rs` if practical.
- Rename `tree/client_node_tree.rs` to `tree/node_tree_view.rs`.
- Rename `tree/client_tree_entry.rs` to `tree/tree_entry_view.rs`.
- Rename `prop/wire_prop_access.rs` to `prop/prop_access_view.rs`.
- Update module declarations and crate-root exports.

Rename types:

- `ClientProjectView` -> `ProjectView`
- `ClientNodeEntry` -> `NodeEntryView`
- `ClientNodeTree` -> `NodeTreeView`
- `ClientTreeEntry` -> `TreeEntryView`
- `WirePropAccess` -> `PropAccessView`
- `WirePropsMap` -> `PropsMapView`
- `StatusChange` -> `StatusChangeView`

Update call sites across:

- `lp-core/lpc-view/src/**`
- `lp-app/**`
- tests and examples that use the client/view cache

Search targets:

```bash
rg "ClientProjectView|ClientNodeEntry|ClientNodeTree|ClientTreeEntry|WirePropAccess|WirePropsMap|StatusChange" lp-core/lpc-view lp-app
```

Expected result:

- Old view/cache names do not appear in active Rust code.
- `ClientApi` remains only if it is genuinely client/API-facing.

# Validate

Run:

```bash
cargo +nightly fmt
cargo check -p lpc-view
cargo test -p lpc-view
```
