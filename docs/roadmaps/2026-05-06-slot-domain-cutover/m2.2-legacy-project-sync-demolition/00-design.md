# M2.2 Legacy Project Sync Demolition Design

## Scope Of Work

M2.2 removes the old project sync/detail/debug UI path from active code after
tag `2026-05-07-pre-legacy-remove`. The reference checkout at
`/Users/yona/dev/photomancer/lp2025` preserves the old implementation for
comparison.

In scope:

- Delete legacy project response/detail/state wire types from the active path.
- Delete legacy node detail projection from the engine.
- Rename useful retained data structures away from compatibility/legacy names.
- Disable project sync entry points that cannot be canonical until M3.
- Gut the old node-specific debug UI.
- Remove legacy-detail-only tests.
- Preserve useful resource summary/payload machinery.

Out of scope:

- Canonical project sync protocol design and implementation.
- Rebuilding `ProjectView` around `SlotMirrorView`.
- Rebuilding the generic debug UI.
- Runtime state/params/output slot roots.
- Client-driven mutation.

## File Structure

```text
lp-core/lpc-wire/src/
  legacy/                         # delete or remove from active exports
  project/
    legacy_wire_node_specifier.rs  # delete
    wire_project_request.rs        # remove legacy detail request shape or disable sync request
    resource_sync.rs               # keep, update docs away from legacy GetChanges
  server/api.rs                    # keep generic ServerMsgBody<R>
  message/client.rs                # keep envelope, adjust tests

lp-core/lpc-engine/src/project_runtime/
  source_projection.rs             # rename/reframe compatibility_projection.rs
  detail_projection.rs             # delete
  core_project_runtime.rs          # remove legacy get_changes
  resource_projection.rs           # keep useful resource projection helpers
  project_loader.rs                # record source authoring snapshots under new name

lp-core/lpc-view/src/project/
  project_view.rs                  # remove legacy detail model or reduce to minimal shell
  resource_cache.rs                # keep cache, remove LegacyCompatBytes helpers

lp-app/lpa-server/src/
  handlers.rs                      # disable project sync handling until M3

lp-app/lpa-client/src/
  client.rs                        # remove legacy sync conversion, keep transport/load/unload basics

lp-cli/src/debug_ui/
  nodes/                           # delete
  panels.rs                        # delete
  ui.rs                            # minimal empty/FPS shell or disabled entry
```

## Architecture Summary

M2.2 is a controlled demolition milestone. It removes code whose purpose is
feeding or rendering legacy `NodeDetail` / `NodeState`, while preserving
transport, source loading, resource sync primitives, and slot infrastructure.

The canonical project sync protocol is not introduced here. Instead, project
sync entry points that previously returned legacy detail data become explicitly
disabled with TODOs pointing to M3. This keeps the workspace compiling without
inventing placeholder canonical behavior too early.

The old `CompatibilityProjection` name is misleading. The useful concept is a
runtime node id to source authoring snapshot/path index. M2.2 should rename and
trim it so M3 can use it for canonical source root sync.

Resource sync is not legacy in spirit. Summary and explicit payload request
types should survive, with comments updated so they do not refer to legacy
`GetChanges` as the owner of the model.

The debug UI is not ported. It is gutted or replaced with a minimal shell until
M5 rebuilds it as a generic slot/resource inspector.

## Main Components And Interactions

- Wire envelopes continue to serialize client/server messages.
- Project-specific sync requests are removed or disabled until M3 defines the
  canonical response type.
- Engine project runtime keeps loading/ticking projects but no longer projects
  legacy detail state.
- Source authoring snapshots remain indexed by `NodeId` for future source slot
  sync.
- Client and view stop converting/applying legacy project responses.
- Debug UI no longer renders node-specific legacy state.

