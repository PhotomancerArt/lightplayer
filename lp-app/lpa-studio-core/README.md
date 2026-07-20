# lpa-studio-core

`lpa-studio-core` is the headless LightPlayer Studio application core.

This crate sits above lower-level services such as `lpa-link` and
`lpa-client`, owns those services for Studio, and exposes the user-shaped
language consumed by renderers: views, actions, progress, issues, logs, and
project summaries.

The current stateful controller types still use the `*Ux` suffix
(`StudioUx`, `DeviceUx`, `ProjectUx`) for compatibility with the existing app
surface. The architectural role is "controller": these types own Studio app
state and policy, accept typed operations, and produce inert view data.

The UI layer should render this language and dispatch actions back into
`StudioUx`. It should not own provider runtimes, drain service effects,
correlate protocol responses, or implement project attach/load policy.

```text
lpa-link / lpa-client / lp-server protocol
        owned by
lpa-studio-core
        rendered by
lpa-studio-web, future CLI, future desktop, tests, and agents
```

## Boundaries

- `lpa-link` owns provider resources such as browser workers, endpoint/session
  identity, and device/runtime management.
- `lpa-client` owns server protocol request ids, response correlation, typed
  project operations, and side-channel protocol events.
- `lpa-studio-core` owns Studio product state, the `LinkProviderRegistry`, the
  connected server client, project attach/load policy, and async action
  execution above those services.
- `lpa-studio-web` renders `StudioView` panes, stack sections, terminal output,
  and available actions.

## Source Layout

- `core/` contains reusable data-driven app substrate: action metadata, generic
  pane/stack/activity/status view data, and UX node routing primitives.
- `app/` contains the actual Studio product ownership areas: `studio`,
  `device`, `server`, `project`, and `preview_host` (leased, pooled live
  project previews for gallery cards and future preview consumers — see
  `docs/adr/2026-07-16-preview-host.md`; the browser-facing half is gated
  to `wasm32 + browser-worker` like the server client io, while its
  request/status/scheduling vocabulary stays target-neutral and
  unit-tested).
- A future `base/` layer can hold truly primitive app-core concepts if one
  emerges. It is intentionally not present until there is a clean need for it.

The long-term naming direction is role-based: render data should move toward
`*View` names, stateful owners toward `*Controller` names, snapshots remain
`*Snapshot`, and command payloads remain `*Op`. This crate keeps the legacy
`Ui*` and `*Ux` public names for now so the high-level crate/layer refactor
does not blur into a larger API rename.

## Public Model

- `StudioUx` is the top-level controller. It owns `DeviceUx` and `ProjectUx`.
- `DeviceUx` is the user-facing device workflow. It owns the lower-level link
  and server controllers and presents one stack of steps: select connection,
  connect device, connect LightPlayer, and open project. The stack is
  progressive: completed steps remain as compact history, the current relevant
  step owns the available actions, and future steps are hidden until they are
  useful.
- Device exposes the open-project step only after LightPlayer is connected. That
  step offers running-project attach and demo-load actions until a project is
  loaded.
- `DeviceController` also owns the connect flow: the `LinkProviderRegistry`
  catalog, the picker view state (`ConnectFlowState`), and the runtime
  attachment (a hardware `DeviceSession` or the simulator's worker session).
- `ServerUx` owns the connected `lpa-client` protocol client once a link exposes
  server I/O. It remains an implementation detail below `DeviceUx`.
- `ProjectUx` owns Studio's view of the loaded project and is shown only after a
  project is loaded. It keeps the internal `lpc-view::ProjectView` mirror in
  sync with server project reads and exposes semantic readonly project-editor
  views to UI code. The web UI does not own or inspect the raw `ProjectView`.
- `UxNodeId` is a path-shaped UX address with dotted display compatibility.
  Static ids such as `studio.device` still compare and render as strings, while
  dynamic editor ids can be built structurally with child segments.
- The UX ownership tree and address tree are related but not identical. A
  dynamic address such as `studio.project.node_tree` or
  `studio.project.node.4.slot.brightness` does not imply that Studio owns a
  separate boxed node object for that target.
- Dispatch is hierarchical. `StudioUx` routes top-level device actions to
  `DeviceUx` and routes `studio.project` plus `studio.project.*` actions to
  project ownership. `ProjectUx` owns interpretation of project-local targets
  such as `node_tree`, `node`, `slot`, `asset`, `changes`, and `bus`.
- `UiAction` is an in-process action offering: target `UxNodeId`, boxed typed
  operation, and metadata such as label, summary, priority, icon, enablement,
  and confirmation.
- `DeviceOp` and `ProjectOp` are the typed user-facing operations. Operation
  identity is the enum type and variant, not a parallel string action kind.
- `ProjectEditorTarget` and `ProjectEditorOp` are the first project-editor
  dynamic target/op pair. They prove dynamic routing while staying deliberately
  small; real node, slot, binding, bus, and asset behavior belongs to later
  editor milestones.
- `StudioView` is the semantic render surface. It contains a Device
  `UiPaneView`, an optional loaded Project `UiPaneView`, and recent logs.
- `UiBody` is intentionally small: text, progress/activity, issue, metrics,
  stack, project editor, or empty. It is not a generic component schema.
- `UiStackView` / `UiStackSection` model reusable multi-step product workflows.
  Device uses them for connection, LightPlayer attach, provisioning, and project
  opening. Section-local actions are the action surface.
- `UiActivity` describes live work inside a pane or stack section: title,
  optional progress, optional milestone steps, and optional terminal lines.
- `UxUpdate` / `UxUpdateSink` let `StudioUx::dispatch_with_updates` publish
  live pane activity or fresh `StudioView` snapshots while an async action is
  still running.
- `StudioSnapshot` and the node snapshots remain cloneable domain read models,
  but web rendering should prefer `StudioView`.

The first slice supports the browser-worker simulator and browser Web Serial
ESP32 entrypoints. It launches `fw-browser` through `lpa-link`, talks to the
real `lp-server` protocol through `lpa-client`, attaches to a running project
when one is already loaded, can load the demo project, and reads project
inventory.

Project data sync is also core-owned. After Studio attaches to a running project
or loads the demo project, `ProjectUx` performs a shape-registry sync followed
by a normal project read for node detail, initial slot roots, resource
summaries, and runtime status. The loaded Project pane shows a compact summary
of the synced mirror alongside a readonly node workspace and exposes
`Refresh project` for explicit action-driven refreshes. `ProjectSync` keeps the
raw `lpc_view::ProjectView` private and translates it into `ProjectEditorView`,
`ProjectNodeTreeView`, `ProjectNodeView`, and `ProjectSlotRowView` data before
anything reaches a UI. Sync failures are treated as project-pane issues rather
than generic action failures so the attached project can stay visible while
Studio explains what needs attention.

## Client Sync Engine

Since M7 the client update logic lives here, not in the UI crate. A renderer no
longer owns the controller, drives timeouts, or holds preemption/timeout policy;
it enqueues commands and renders change-gated snapshots. The pieces:

- **`StudioActor` owns the `StudioController`.** One task owns the controller;
  every input — user gestures and the UI's refresh timer — arrives as a
  `StudioCommand { Action, RefreshTick, Shutdown }` on an ordered queue.
  `StudioActor::new(controller, make_timer)` returns the actor plus a
  `StudioHandle`; the caller drives `StudioActor::run` (wasm: under
  `spawn_local`; native: a future tokio helper). This retires the web crate's
  `Option<StudioController>` take/put, generation counters, cancel flags, and
  25 ms spin loops — preemption is now queue priority.
- **`StudioHandle` is the entire UI boundary:** `tx` to enqueue commands, `view`
  (a change-gated `UiStudioView` channel the UI drives into a signal), and a
  shared next-tick `delay`. The actor coalesces redundant `RefreshTick`s to one
  pull, runs pending actions ahead of ticks, and emits **one** snapshot per
  batch only when the view actually changed (revision advanced or a local
  mutation set the dirty flag).
- **Op policy is data (`ActionClass`).** Each op maps to `ActionClass {
  Recovery, Foreground { deadline }, Passive { deadline } }` beside its
  definition (`ControllerOp::action_class` / `UiAction::class`). The actor reads
  it to decide whether an incoming action preempts an in-flight passive pull and
  to build the pull loop's quiet-gap `ProgressDeadline`. A new op must declare a
  class — a compile error, not a silent default. The retired web match-table
  functions and their per-transport wall-clock constants are gone.
- **Passive pulls are cancellable and deadline-bounded.** `run_refresh_tick`
  drives one gated read through `lpa-client`'s pull loop under a `ProgressDeadline`
  (quiet-gap budget, reset per frame) and a `CancelSignal` the actor flips when a
  preempting command arrives; a clean cancel is not a failure, a timeout/error
  applies `BackoffPolicy`. Progressive `UxUpdate` activity/log deltas during a
  long action are applied to the live view (`UiStudioView::apply_activity`) and
  republished through the same channel.
- **Cadence is data (`RefreshCadence`).** The refresh interval the UI timer waits
  is derived in core from the connection's `ConnectFlowState` (`RefreshCadence::for_flow_state`)
  and surfaced through `StudioHandle::next_refresh_delay` (interval + backoff), so
  no `LinkProviderKind` transport-sniffing lives in the view layer. The simulator
  keeps a faster interval only because it self-ticks and the UI re-reads previews
  at that rate.
- **Request scoping** stays core-owned: the focus-scoped probe set
  (`node_subscribes_products`) is picked up by the next pull; `Focus` completes
  synchronously with no bolt-on network refresh.

The client's single timeout/cancel/retry owner and the actor model are recorded
in `docs/adr/2026-07-04-client-pull-loop-and-actor.md`.

The client treats passive project refresh as lower priority than foreground
device/server recovery. If a browser refresh is interrupted or times out, Studio
marks the project sync as needing attention so device actions can run.
Control-product probes can also be disabled for the current sync when a timeout
suggests older firmware does not understand the newer probe request shape.

The first editor view renders every synced node in stable tree order rather
than requiring a selected-node detail view. Node bodies show headers, status,
prominent `input`/`output` slots, config/state slot rows, compact bindings when
available, and secondary project/runtime stats. Binding authoring, bus views,
and asset editing are later milestones; the editing substrate itself is below.

## Edit Model

Slot editing state is core-owned end to end; UI field components are
stateless views that dispatch ops and render DTOs. The model (recorded in
`docs/adr/2026-07-04-studio-editing-model.md`) has four pieces:

- **Ops.** `SlotEditOp { SetValue, EnsurePresent, RemoveValue, MoveEntry,
  Revert }`
  target one `ProjectSlotAddress` (carried on the op — no per-slot controller
  ids); `NodeRevertOp { node }` batch-reverts every edit entry under a node's
  subtree (the controller expands it into one `RemoveSlotEdit`-per-entry
  mutation batch — one wire round-trip, one snapshot);
  `ProjectOp::SaveOverlay` commits the overlay and
  `ProjectOp::RevertAllEdits` clears it. All are `ActionClass::Foreground` on
  the 6 s editor quiet-gap deadline. The actor coalesces consecutive queued
  `SetValue`s per address latest-wins (`push_action_coalesced`), so `oninput`
  floods collapse to one mutation; any other action — including the
  structural gestures — is a barrier and never coalesces. Composite gestures
  ARE the wire ops: map add / option on / enum variant switch dispatch
  `EnsurePresent`, map entry remove / option off dispatch `RemoveValue`, and
  the server constructs all defaults. `MoveEntry { address(map), from_key,
  to_key }` re-keys one map entry: it sends `MutationOp::MoveSlotEntry`
  (keys are path segments, so this is its own mutation, not a value edit),
  stages at the map's own address, and the ack's
  `MutationEffect::Materialized` lists the per-path edits the server stored
  (ensure target + diverged-leaf assigns + source remove), which the mirror
  replays verbatim; an occupied target rejects with `target_occupied`.
  `UiConfigSlot.composite`
  (`UiSlotComposite::{Map, Enum}`) carries the map key domain + the
  gap-filling first-free suggested key (numeric maps add there immediately;
  the key input is an override) and the declared enum variant idents (raw,
  verbatim) that the gesture affordances render from.
- **Edit buffer.** `ProjectController` holds a path-keyed buffer of
  `PendingEdit`s (`slot/pending_edit.rs`, state machine documented on the
  type). A buffered value shadows the synced value in DTOs from field input
  until the server **acks** the covering mutation — not merely until blur or
  the next pull — which is what prevents rubber-banding at device pull
  cadence. Accept releases the entry into the overlay mirror
  (`ProjectSync::apply_acked_edits`); reject/transport failure parks it as
  `Failed`, feeding the field's `invalid` reason until the next edit or a
  revert.
- **Overlay mirror.** `ProjectSync` mirrors the server's pending-edit
  overlay, revision-gated: the runtime status carries `overlay_changed_at`
  on every pull, and the mirror issues one full `ReadOverlay` ride-along
  only when it advanced (a quiet-but-dirty project fetches nothing).
- **Dirty derivation.** A slot is dirty iff the overlay contains an edit at
  its path — no client-local dirty tracking, so dirty state is
  cross-client-correct and survives reconnects. The DTO join
  (`slot/slot_edit_join.rs`): buffer entries map to `Saving`/`Error`,
  overlay-mirror entries to `Dirty`; `UiSlotFieldState.live` distinguishes
  transient ("live") from persisted ("unsaved") edits. The same join feeds
  `DirtySummary { persisted, transient, failed }` (`project/dirty_summary.rs`),
  aggregated slot → node → project during the DTO build: node headers,
  child entries, sidebar tree items, and `ProjectEditorView.dirty` all carry
  it, and the project header's contextual Save/Revert actions surface as
  controller-produced `UiPaneAction`s on `ProjectEditorView.header_actions`;
  dirty node headers likewise carry the subtree batch revert
  (`NodeRevertOp`) on `UiNodeView.header_actions` / `UiNodeChild.header_actions`.
  Each hierarchy DTO also projects status + dirty into its one chrome
  `UiAffordance` (`project/ui_affordance.rs`, priority merge
  Error > Unsaved > Live > Busy > Info) — the glyph/tone every detail
  trigger and tree-row indicator renders.
  `UiConfigSlot` carries its `ProjectSlotAddress` so fields can dispatch
  edits without extra lookup, plus `edit_entry_address` — the row's **own**
  edit entry when one exists (the row-level Revert/Reset target; a
  prefix-only-dirty composite carries none; a present option row also owns
  its interior `.some` entry, and an enum row owns a variant-child entry —
  the variant-switch gesture's storage path). The project popup's save panel
  renders `ProjectEditorView.pending_edits`: one `UiPendingEdit` per edit
  entry (node label, slot path, op/value display string, phase
  persisted/live/failed, per-entry revert action), built from the same join
  enumeration the counts sum — list and counts cannot drift. Each
  `UiPendingEdit` also carries `old_value` (the saved base as a display
  string) so popups and the panel render `old → new`; base values are
  server-derived and mirrored beside the overlay (`ProjectSync::
  base_value_at`, pruned to the overlay's paths). The project root's own
  slots ride `ProjectEditorView.root_slots` (rendered in the project pane,
  not as a workspace card); `format`/`nodes` are `read_only_persisted`,
  `name` stays writable.

**Asset bodies** (ADR D8) extend the same model to whole files
(`project/asset/`): `AssetEditOp::{ApplyBody, Revert}` stage
`SetArtifactBody` / `ClearArtifact` mutations through an artifact-keyed
sibling buffer with the identical ack lifecycle, joined into the same dirty
summaries and save-panel rows (`UiPendingEditKind::AssetBody`; a `.glsl`
that maps to no synced node still counts via
`unmapped_asset_dirty_summary`). `ApplyBody` enforces the client-side
`MAX_ASSET_BODY_BYTES` (10 KB) guard under the 16 KB wire frame budget.
Effective editor content resolves buffer → overlay mirror → cached base
body (`asset_content`, fetched via `StudioFsRead` on demand and invalidated
by save/revert); the per-slot editor DTO is `UiAssetEditor`, embedded on
`UiSlotAsset.inline_editor` for every editable asset slot in the node walk
(`embed_asset_editors`, recursing records for nested assets), so an editor
appears inline wherever an editable asset does. `apply_action(text)` is the
Apply mutation; the current text and modified flag are the one deliberately
editor-local piece of state (the web component owns them — see the ADR).
Compile-error status text parses best-effort into `UiShaderError`
(message + optional `line:col` from the rustc-style marker) for the
editor's error strip and gutter.

Project attach behavior is core-owned:

- zero loaded projects: once LightPlayer is connected, offer to load the demo
  project in the Device open-project step;
- one loaded project: auto-attach after server connection and then show the
  Project pane;
- multiple loaded projects: show the selection in the Device open-project step
  and expose one action per loaded project.

For the browser-worker simulator, the zero-loaded-project case auto-loads the
demo project. Real hardware remains conservative and requires explicit project
loading when nothing is running.

## Feedback And Recovery

Recoverable connection problems are modeled in the same view/action language as
the rest of Studio. If opening a device fails, the connect flow returns to provider
selection with an inline `UxIssue` and the normal provider actions still
available. Retrying is therefore the same operation as choosing a connection
again; `Refresh connections` is reserved for rebuilding the provider catalog.

Canceling the browser Web Serial chooser is a normal UX outcome, not a failed
link. `lpa-link` preserves chooser cancellation as a typed cancellation error,
the connect flow returns to provider selection without an issue, and `StudioUx`
reports only a low-key notice suitable for a console or activity log.

Generic notices and action failures are expected to flow into recent activity
logs. Actionable issues that affect the next user choice should live inline in
the relevant `UiStackSection` body.

## Console Logging

The console model lives in `core/log/` (ADR
`2026-07-05-studio-logging-model`):

- `UiLogEntry` carries a timestamp (`f64` seconds since Unix epoch, stamped at
  push time by a clock injected into `StudioController`), a severity
  (`Trace..Error`, ordered), and a `UiLogSource { origin, detail }` — origin is
  the closed enum `Studio | Link | Server | Device`; detail is optional free
  text (module path, endpoint id). Producers hand the controller unstamped
  `UiLogDraft`s.
- `LogRing` keeps the last 1000 entries unfiltered. `LogFilter` (min-level
  threshold, default Info+, plus per-origin toggles) is applied display-side
  when building `UiConsoleView`, so relaxing the filter reveals captured
  history. Filter mutations arrive as `StudioCommand::Console(ConsoleCommand)`
  — synchronous state changes, not `UiAction`s.
- Healthy server heartbeats are telemetry, not log entries; recovery/safe-mode
  conditions still log as Warn/Error. Firmware serial lines are parsed from
  the `[LEVEL] module: message` format into structured entries.
- Studio-side code logs through the standard `log` macros: a global sink
  (`core/log/log_sink.rs`) buffers records in a bounded thread-local queue
  that the studio actor drains into the ring each batch/tick (origin Studio,
  target as detail). The controller's `on_entry` hook is the single
  JS-console mirroring point.
- `DeviceOp::SetLogLevel` sets the connected server's runtime verbosity over
  the wire (`ClientMsgBody::SetLogLevel` → `log::set_max_level`); the console
  toolbar sends it as `ConsoleCommand::SetDeviceLogLevel`, converted to the
  device action at actor intake. Not persisted device-side; tracked
  optimistically per connection.

## Device Management UX

Blank-device provisioning and recovery are modeled as Device actions backed by
link-level management because they happen below the running server protocol:

- `Flash firmware` is offered in the Connect LightPlayer step when the
  connected device session supports `FlashFirmware` and Studio is not currently
  attached to a server.
- `Wipe device` is offered as a tertiary destructive Device action when the
  connected device session supports `EraseDeviceFlash`.

Both actions flow through `lpa-link::LinkProvider::manage_with_events`.
`StudioUx` clears project and server state before executing them because
firmware flashing and full-device erase invalidate any previous server/client
connection. Browser Web Serial ESP32 management streams progress into the
active Device step and raw esptool output into the Studio log while the action
is running.

After provisioning, Studio attempts to reopen the server protocol and resume the
normal server/project workflow. If the browser or device needs more time after
reset, Studio keeps the link context and reports that the user should reconnect
after boot.

For Browser Web Serial ESP32 links, opening or reopening the server protocol
goes through the provider-owned browser ESP32 device controller. The controller
opens the Web Serial port once, starts reading immediately, then attempts a
best-effort reset while raw boot output is being captured. The browser-serial
client waits for either a valid protocol frame or the firmware's server-loop
startup line before sending the first request, so a just-reset device does not
lose the initial project probe while firmware is still booting. Reset signal
failures are reported as diagnostics; the user-facing readiness result comes
from raw serial output and protocol frames.

While waiting for browser serial readiness, Studio publishes a stepped
`UiActivity` in the Device pane. The reusable activity data includes serial
access, device reset, boot output, and LightPlayer protocol readiness; raw boot
lines are emitted as logs so the web UI, agents, and future CLI shells can
render progress and logs in separate places.

If ESP32 boot output includes patterns such as `invalid header: 0xffffffff`,
Studio classifies the device as blank/erased instead of surfacing a generic
protocol timeout. The link session remains open, project/server state is
detached, and `Flash firmware` remains available when the selected provider
advertises flashing support.

After wipe, Studio leaves project and server detached and returns to a link
state that can offer firmware flashing again. Wipe is not a server filesystem
clear; it is a destructive whole-device erase through the link provider.

Disconnect semantics are intentionally distinct:

- disconnecting a project detaches Studio from the project and leaves the server
  and link connected;
- disconnecting the Device clears project/server/link and returns to connection
  choices.

## Agent And CLI Use

The same tree can be rendered textually for agents or future CLI shells:

```rust
let view = studio.view();
let text = view.render_text();
```

Actions remain in-process values. Text rendering can describe available actions,
but it is not a stable wire protocol and does not serialize operations.
Interactive shells can use `dispatch_with_updates` to show progress/terminal
updates during long actions without owning provider resources themselves.

There is intentionally no central `UxRegistry` object yet. The current Studio
tree is naturally hierarchical, so each owner consumes its own target subtree.
If Studio later needs non-tree mounting, plugin-style routes, or cross-cutting
introspection, a registry can be introduced on top of the path-shaped
`UxNodeId` model without changing action identity.

## Naming Note

An earlier archived design used a separate core/runtime split. The active
workspace now uses this single `lpa-studio-core` crate for Studio controller
logic, app policy, view data, and live updates instead of routing application
work through a separate effect/event executor.

## Validation

```bash
cargo check -p lpa-studio-core
cargo test -p lpa-studio-core
cargo check -p lpa-studio-core --target wasm32-unknown-unknown --features browser-worker,browser-serial-esp32
```
