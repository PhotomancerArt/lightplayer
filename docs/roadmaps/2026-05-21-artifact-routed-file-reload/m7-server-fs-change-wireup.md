# Milestone 7: Server Fs-Change Wire-Up

## Title And Goal

Route filesystem notifications to **`Engine::handle_fs_changes`** on the
**post-M6** stack and stop **`Project::reload()`** on watcher events.

## Prerequisites

Parallel build ends at **M6**. Engine uses `lpc-node-registry` + ChangeSet model
from M5.

## Suggested Plan Location

`docs/roadmaps/2026-05-21-artifact-routed-file-reload/m7-server-fs-change-wireup/`

## Scope

In scope:

- `LpServer` → `Engine::handle_fs_changes`.
- End-to-end single-file reload (TOML, assets).
- Explicit `Project::reload()` retained for user-initiated full reload.

Out of scope:

- `project.toml` graph reconciliation (**M8**).

## Dependencies

- M6 engine cutover.

## Execution Strategy

Small plan. Narrow server→engine wiring.

Suggested chat opener:

> I suggest a small plan for server fs-change wire-up, then implement. Agree?
