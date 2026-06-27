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
  `device`, `link`, `server`, and `project`.
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
- `LinkUx` owns link-provider selection, the `LinkProviderRegistry`, and the
  active link session. It remains an implementation detail below `DeviceUx`.
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

The web shell treats passive project refresh as lower priority than foreground
device/server recovery. If a browser refresh is interrupted or times out, Studio
marks the project sync as needing attention and returns the controller to the UI
so device actions can run. Control-product probes can also be disabled for the
current sync when a timeout suggests older firmware does not understand the newer
probe request shape.

The first editor view renders every synced node in stable tree order rather
than requiring a selected-node detail view. Node bodies show headers, status,
prominent `input`/`output` slots, config/state slot rows, compact bindings when
available, and secondary project/runtime stats. Editing, overlay dirty-state,
binding authoring, bus views, probes, and asset editing are later milestones.

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
the rest of Studio. If opening a device fails, `LinkUx` returns to provider
selection with an inline `UxIssue` and the normal provider actions still
available. Retrying is therefore the same operation as choosing a connection
again; `Refresh connections` is reserved for rebuilding the provider catalog.

Canceling the browser Web Serial chooser is a normal UX outcome, not a failed
link. `lpa-link` preserves chooser cancellation as a typed cancellation error,
`LinkUx` returns to provider selection without an issue, and `StudioUx` reports
only a low-key notice suitable for a console or activity log.

Generic notices and action failures are expected to flow into recent activity
logs. Actionable issues that affect the next user choice should live inline in
the relevant `UiStackSection` body.

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
