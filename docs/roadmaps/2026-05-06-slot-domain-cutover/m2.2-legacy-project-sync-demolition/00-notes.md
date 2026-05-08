# M2.2 Legacy Project Sync Demolition Notes

## Scope Of Work

This milestone deletes the old project sync/detail/debug UI path after tag
`2026-05-07-pre-legacy-remove` preserved it for reference. The point is to stop
threading new slot-domain work through legacy project response shapes.

In scope:

- Remove active `LegacyProjectResponse` / `LegacySerializableProjectResponse`
  project sync.
- Remove active legacy node detail request/response vocabulary.
- Remove engine legacy detail projection.
- Remove or disconnect old debug UI panels that render `LegacyNodeState`.
- Remove client/view code whose only purpose is applying legacy detail state.
- Keep resource summary/payload code that should move into canonical sync.
- Keep real source/engine domain loading code, even if legacy detail previously
  consumed it.

Out of scope:

- Designing the final canonical project sync protocol. That is M3.
- Rebuilding `ProjectView` around `SlotMirrorView`. That is M4.
- Rebuilding the generic debug UI. That is M5.
- Exposing runtime state/params/output slot roots. That is M6.
- Client-driven mutation.

## User Notes

- The old path is not used by external users.
- The old UI and messages are getting rebuilt anyway.
- The legacy path was kept around until now as a reference for the desired
  behavior, but it now complicates the namespace and design.
- Desired manual strategy:
  1. tag here, check it out in a worktree for reference,
  2. delete all legacy stuff: UI, messages, everything,
  3. rebuild one layer at a time: messages, client, UI.
- Tag `2026-05-07-pre-legacy-remove` already exists.
- Backup checkout/reference worktree is available at
  `/Users/yona/dev/photomancer/lp2025` if deleted legacy code needs to be
  consulted.
- Resource sync should not be casually deleted; summaries and explicit payload
  interest remain conceptually correct.

## Current Code State

### Wire

- `lp-core/lpc-wire/src/legacy/project/api.rs` defines:
  - `LegacyProjectResponse`
  - `LegacyNodeChange`
  - `LegacyNodeDetail`
  - `LegacyNodeState`
  - `LegacySerializableNodeDetail`
  - `LegacySerializableProjectResponse`
- `lp-core/lpc-wire/src/legacy/nodes/*/state.rs` defines the old node-specific
  state wire shapes.
- `lp-core/lpc-wire/src/project/legacy_wire_node_specifier.rs` defines
  `LegacyWireNodeSpecifier`.
- `lp-core/lpc-wire/src/project/wire_project_request.rs` still models
  `WireProjectRequest::GetChanges` with `legacy_detail_specifier`,
  `slot_watch_specifier`, resource summary interest, runtime-buffer payload
  interest, and render-product payload interest.
- `lp-core/lpc-wire/src/server/api.rs` is generic over the project response body:
  `ServerMsgBody<R>::ProjectRequest { response: R }`. This generic envelope is
  useful and should probably survive.
- `lp-core/lpc-wire/src/message/client.rs` uses `WireProjectRequest`, but the
  message envelope itself is not legacy-specific.
- `lp-core/lpc-wire/src/legacy/mod.rs` currently exports aliases:
  `LegacyMessage`, `LegacyServerMessage`, and `LegacyServerMsgBody`, all tied
  to `LegacySerializableProjectResponse`.
- `lp-core/lpc-wire/src/project/resource_sync.rs` contains the useful resource
  summary / explicit payload request/response vocabulary. Its docs still point
  at legacy `GetChanges`, but the types themselves should likely move forward.

### Engine

- `lp-core/lpc-engine/src/project_runtime/core_project_runtime.rs` has
  `CoreProjectRuntime::get_changes(...) -> LegacyProjectResponse`.
- `get_changes` currently builds:
  - node handle list,
  - `LegacyNodeChange` lifecycle/status events,
  - `node_details` through `build_node_detail_map`,
  - theoretical FPS,
  - resource summaries,
  - runtime-buffer payloads,
  - render-product payloads.
- `lp-core/lpc-engine/src/project_runtime/detail_projection.rs` builds
  `LegacyNodeDetail` / `LegacyNodeState` from runtime nodes and compatibility
  authoring config.
- `lp-core/lpc-engine/src/project_runtime/compatibility_projection.rs` stores
  `NodeId -> LoadedNodeConfig` and `NodeId -> LpPathBuf` for legacy detail
  construction.
- `project_loader.rs` records authoring snapshots into `CompatibilityProjection`
  while loading nodes. This data may be useful for canonical source-root sync
  if renamed/reframed.
- `project_runtime/resource_projection.rs` already contains useful projection
  logic for resource summaries and explicit payloads. It should be kept unless
  M3 redesigns the boundary.

### Server And Client

- `lp-app/lpa-server/src/handlers.rs` handles
  `WireProjectRequest::GetChanges`, calls `CoreProjectRuntime::get_changes`,
  converts to `LegacySerializableProjectResponse`, and returns generic
  `ServerMsgBody::ProjectRequest { response }`.
- `lp-app/lpa-client/src/client.rs` imports
  `LegacySerializableProjectResponse` / `LegacyServerMessage`.
- `LpClient::project_sync_internal` sends legacy `GetChanges` and returns
  `LegacySerializableProjectResponse`.
- `serializable_response_to_project_response` reconstructs
  `LegacyProjectResponse` from the serializable response for `ProjectView`.

### View

- `lp-core/lpc-view/src/project/project_view.rs` is legacy-shaped:
  - owns typed `Box<dyn NodeDef>` config mirrors,
  - owns `Option<LegacyNodeState>`,
  - tracks `legacy_detail_tracking`,
  - applies `LegacyProjectResponse`,
  - provides `get_texture_data` and `get_output_data` through legacy state.
- `lp-core/lpc-view/src/project/resource_cache.rs` is mostly useful, but it
  depends on `LegacyCompatBytesField` for helper functions that resolve old
  heavy byte fields.
- `lpc-view::slot::SlotMirrorView` already exists and should become the
  canonical client-side data mirror in M4, not M2.2.

### Debug UI

- `lp-cli/src/debug_ui/ui.rs` synchronizes
  `LegacySerializableProjectResponse`, converts it, and applies it to
  `ProjectView`.
- `lp-cli/src/debug_ui/panels.rs` renders node-specific panels from
  `LegacyNodeState` and legacy detail tracking.
- `lp-cli/src/debug_ui/nodes/*` are node-specific legacy detail panels.
- M2.2 should remove or disconnect this UI rather than port it. M5 rebuilds a
  generic slot/resource inspector.

### Tests

Legacy sync/detail tests are spread through:

- `lp-core/lpc-wire/src/legacy/project/api.rs` unit tests.
- `lp-core/lpc-wire/tests/m4_get_changes_all_specifiers_roundtrip.rs`.
- `lp-core/lpc-view/tests/client_view.rs`.
- `lp-core/lpc-engine/tests/scene_update.rs`.
- `lp-core/lpc-engine/tests/partial_state_updates.rs`.
- `lp-core/lpc-engine/tests/scene_render.rs`.
- `lp-core/lpc-engine/tests/get_changes_resource_projection.rs`.
- `lp-app/lpa-client/tests/scene_render_emu_async.rs`.
- `lp-app/lpa-server/tests/server_tick.rs`.

Many of these should be deleted or marked for M3/M4 rewrite rather than kept
green by preserving legacy detail.

## Open Questions

### Q1. Should M2.2 Leave The Workspace Temporarily Broken?

Context: the milestone is intentionally destructive and M3/M4 rebuild the
canonical sync/view path. Deleting `LegacyProjectResponse` and old debug UI will
break `lpa-server`, `lpa-client`, `lpc-view`, `lp-cli`, and several tests until
canonical replacements exist.

Suggested answer: allow M2.2 to leave broad project-sync crates broken, but keep
the low-level crates that are not part of the demolition (`lpc-model`,
`lpc-source`, slot mockup, resource primitives) green. End the milestone with a
compile-fallout inventory that becomes M3 input.

Alternative: keep everything compiling by adding placeholder canonical response
types or temporary no-op sync. This reduces disruption but risks turning M2.2
into an accidental M3 implementation.

User answer: we can break behavior, but the cleanest path is to disable broken
entry points so the workspace still compiles and tests can pass. Delete legacy
code, then comment out or stub module invocations where the canonical
replacement does not exist yet.

### Q2. Does `CompatibilityProjection` Get Deleted Or Renamed?

Context: `CompatibilityProjection` is legacy in purpose today, but the data it
stores, `NodeId -> LoadedNodeConfig` and `NodeId -> LpPathBuf`, is probably the
same data canonical source-root sync needs to snapshot source defs by runtime
node id.

Suggested answer: do not delete the underlying index. Rename/reframe it to a
source-authoring index in M2.2, remove `clone_as_node_config_box`, and stop using
it for `LegacyNodeDetail`. M3 can then use it to expose source roots.

Alternative: delete it completely and let M3 rebuild the source index from
scratch. This is cleaner demolition but may churn code we already know we need.

User answer: probably yes. This milestone should aggressively rename anything
we want to keep, preserve useful concepts, and rely on the reference worktree if
we later want to restore deleted code.

### Q3. How Aggressively Should `lpc_wire::legacy` Be Deleted?

Context: `lpc_wire::legacy` contains project response types, node-specific
state types, and `LegacyCompatBytesField`. The project/detail/state types are
wrong for the new domain model. The compat bytes helper is tied to old state
fields, while the resource summary/payload system outside `legacy` is the part
we want to keep.

Suggested answer: delete `legacy/project`, `legacy/nodes`, and
`LegacyWireNodeSpecifier` from active code. Delete `LegacyCompatBytesField`
unless a resource test still needs it as a bridge; resource refs/payloads should
flow through canonical resource types instead.

Alternative: quarantine `legacy` under a clearly reference-only module until M3.
This avoids a larger deletion diff but keeps the namespace around.

User answer: yes.

### Q4. What Should Happen To The Debug UI During M2.2?

Context: the current debug UI is almost entirely driven by
`LegacySerializableProjectResponse`, `ProjectView::legacy_detail_tracking`, and
`LegacyNodeState` panels. M5 rebuilds a generic slot/resource UI.

Suggested answer: remove the old debug UI module from active builds or replace
the `dev` command's UI entry with a short "generic debug UI not rebuilt yet"
stub. Do not port node-specific panels in M2.2.

Alternative: keep the old UI compiling behind a temporary feature or module.
This conflicts with the goal of deleting legacy scaffolding unless there is a
specific near-term usability need.

User answer: yes. The old UI can be gutted completely. An empty window, FPS-only
window, or equivalent minimal stub is fine if the basic command still needs a
window.

### Q5. Which Tests Should Survive M2.2?

Context: many tests assert legacy detail behavior. Some also test real resource
projection behavior that remains important.

Suggested answer: delete legacy-detail-only tests. Preserve or rewrite resource
projection tests only where they target canonical resource summary/payload types
without depending on legacy node details. Record deleted behavior in M2.2 notes
so M3-M5 can restore coverage intentionally.

Alternative: mark tests ignored or leave them failing as compile-fallout
documentation. That is noisier than deleting tests whose subject is gone.

User answer: yes.
