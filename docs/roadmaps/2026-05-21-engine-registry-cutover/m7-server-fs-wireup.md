# Milestone 7: Server Fs Wire-Up

## Title and goal

Route filesystem watcher events to **`Engine::handle_fs_changes`** (registry
sync) instead of **`Project::reload()`**.

Promoted from [artifact-routed M7](../2026-05-21-artifact-routed-file-reload/m7-server-fs-change-wireup.md).

## Suggested plan location

`docs/roadmaps/2026-05-21-engine-registry-cutover/m7-server-fs-wireup/`

## Scope

**In:** `LpServer` → incremental sync; E2E single-file reload (TOML, GLSL).
Explicit full reload retained for user-initiated reset.

**Out:** Graph reconciliation (M6 if not done — ordering flexible).

## Dependencies

- M5 minimum (engine handles fs sync).

## Execution strategy

**Small plan**
