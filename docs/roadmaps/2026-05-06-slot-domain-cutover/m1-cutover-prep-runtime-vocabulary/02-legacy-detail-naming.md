# Phase 2: Legacy Detail Naming

## Scope Of Phase

In scope:

- Finish the already-started legacy rename pass.
- Make active bridge code visibly legacy in type, field, method, and local variable names where practical.
- Fix stale docs/comments created by mechanical rename.

Out of scope:

- Removing legacy detail sync.
- Renaming legacy node state structs under `lpc_wire::legacy::nodes::*` if their module path is already clear.
- Semantic behavior changes.

## Code Organization Reminders

- Prefer granular files with one main concept per file.
- Keep related functionality grouped together.
- Put helpers lower in the file when that improves readability.
- Mark any temporary code with a clear `TODO`.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

The user already performed most RustRover renames. Audit and finish consistency around:

- `LegacyWireNodeSpecifier`
- `legacy_detail_specifier`
- `LegacyProjectResponse`
- `LegacySerializableProjectResponse`
- `LegacyNodeChange`
- `LegacyNodeDetail`
- `LegacySerializableNodeDetail`
- `LegacyNodeState`
- `ProjectView::legacy_detail_tracking`

Suggested follow-up renames if still present:

- `ProjectView::detail_specifier` -> `legacy_detail_specifier`
- `ProjectView::watch_detail` -> `watch_legacy_detail`
- `ProjectView::unwatch_detail` -> `unwatch_legacy_detail`
- debug UI `tracked_nodes` -> `legacy_detail_nodes`
- debug UI `tracked_nodes_changed` -> `legacy_detail_nodes_changed`
- debug UI `all_detail` -> `all_legacy_detail`

Update comments/docstrings that still say `ProjectResponse`, `NodeDetail`, `NodeState`, or `WireNodeSpecifier` without `legacy` when they refer to the bridge path.

## Validate

```bash
cargo fmt -p lpc-wire -p lpc-view -p lpa-client -p lpa-server -p lp-cli -p lpc-engine
cargo check -p lpc-wire
cargo check -p lpc-view
cargo check -p lp-cli
```

