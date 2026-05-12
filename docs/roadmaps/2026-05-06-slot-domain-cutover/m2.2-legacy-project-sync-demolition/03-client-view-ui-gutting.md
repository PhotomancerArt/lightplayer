# Phase 3: Client, View, And UI Gutting

## Scope Of Phase

In scope:

- Remove legacy project sync conversion from `lpa-client`.
- Remove legacy detail state from `lpc-view::ProjectView`.
- Keep transport/load/unload/filesystem basics where possible.
- Remove or gut old debug UI panels that depend on `LegacyNodeState`.
- Replace debug UI with a minimal empty/FPS shell if needed for CLI commands to
  compile.

Out of scope:

- Rebuilding canonical client project sync.
- Rebuilding generic slot/resource debug UI.
- Porting node-specific panels.
- Client-driven mutation UI.

## Code Organization Reminders

- Prefer deleting obsolete modules over leaving dead code.
- If a command must remain callable, provide a tiny explicit stub instead of
  preserving legacy UI internals.
- Keep helpers lower in files and tests at the bottom.
- Mark temporary stubs with TODOs pointing to M4/M5.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-app/lpa-client/src/client.rs`
- `lp-app/lpa-client/src/lib.rs`
- `lp-app/lpa-client/tests/scene_render_emu_async.rs`
- `lp-app/lpa-server/src/handlers.rs`
- `lp-app/lpa-server/tests/server_tick.rs`
- `lp-core/lpc-view/src/project/project_view.rs`
- `lp-core/lpc-view/src/project/resource_cache.rs`
- `lp-core/lpc-view/src/project/mod.rs`
- `lp-core/lpc-view/src/api/client.rs`
- `lp-core/lpc-view/tests/client_view.rs`
- `lp-cli/src/debug_ui/mod.rs`
- `lp-cli/src/debug_ui/ui.rs`
- `lp-cli/src/debug_ui/panels.rs`
- `lp-cli/src/debug_ui/nodes/**`

Expected changes:

- Remove `serializable_response_to_project_response`.
- Remove `project_sync_internal` return path tied to
  `LegacySerializableProjectResponse`.
- In `lpa-server`, disable `ProjectRequest` sync handling until M3 canonical
  response exists, or return a clear error for project sync requests.
- Simplify `ProjectView` to remove legacy detail tracking, typed config boxes,
  and `LegacyNodeState`.
- Keep `ClientResourceCache` if it no longer depends on `LegacyCompatBytesField`.
- Remove `resolve_legacy_compat_bytes` helpers or rename/rework only if they
  still serve non-legacy resource payload tests.
- Delete `debug_ui/nodes` and `panels.rs`, or remove them from `mod.rs`.
- Replace `DebugUiState` with a minimal app shell that does not sync legacy
  detail.
- Delete legacy-detail-only view/client/server/UI tests.

Edge cases:

- Some CLI commands may construct `ProjectView` only to launch the debug UI.
  Keep a small constructor/API surface if needed, but do not preserve legacy
  detail semantics.
- `lpc-view::slot::SlotMirrorView` should remain untouched except for imports
  required by deleted project modules.

## Validate

Run:

```bash
cargo check -p lpc-view
cargo test -p lpc-view
cargo check -p lpa-client
cargo check -p lpa-server
cargo check -p lp-cli
git diff --check
```

If a crate cannot compile without pulling M3/M4 forward, disable the affected
entry point with an explicit TODO and record it in the cleanup phase.

