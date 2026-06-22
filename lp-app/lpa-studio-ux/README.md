# lpa-studio-ux

`lpa-studio-ux` is the UI-independent Studio UX layer.

`Ux` means a resource-owning product surface. This crate sits above lower-level
services such as `lpa-link` and `lpa-client`, owns those services for Studio,
and exposes a user-shaped language of views, actions, progress, issues, logs,
and project summaries.

The UI layer should render this language and dispatch actions back into
`StudioUx`. It should not own provider runtimes, drain service effects,
correlate protocol responses, or implement project attach/load policy.

```text
lpa-link / lpa-client / lp-server protocol
        owned by
lpa-studio-ux
        rendered by
lpa-studio-web, future CLI, future desktop, tests, and agents
```

## Boundaries

- `lpa-link` owns provider resources such as browser workers, endpoint/session
  identity, and device/runtime management.
- `lpa-client` owns server protocol request ids, response correlation, typed
  project operations, and side-channel protocol events.
- `lpa-studio-ux` owns Studio product state, the `LinkProviderRegistry`, the
  connected server client, and async action execution above those services.
- `lpa-studio-web` renders `StudioView` panes and available actions.

## Public Model

- `StudioUx` is the top-level controller. It owns `LinkUx`, `ServerUx`, and
  `ProjectUx`.
- `LinkUx` owns link-provider selection and the active link session.
- `ServerUx` owns the connected `lpa-client` protocol client once a link exposes
  server I/O.
- `ProjectUx` owns Studio's view of the attached or loadable project.
- `UxAction` is an in-process action offering: target `UxNodeId`, boxed typed
  operation, and metadata such as label, summary, priority, icon, enablement,
  and confirmation.
- `LinkOp`, `ServerOp`, and `ProjectOp` are typed operations. Operation identity
  is the enum type and variant, not a parallel string action kind.
- `StudioView` is the semantic render surface. It contains `UxPaneView` values
  for Link, Server, and Project plus recent logs.
- `UxBody` is intentionally small: text, progress/activity, issue, metrics, or
  empty. It is not a generic component schema.
- `UxActivity` describes live work inside a pane: title, optional progress, and
  optional terminal lines.
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

Project attach behavior is UX-owned:

- zero loaded projects: offer to load the demo project;
- one loaded project: auto-attach after server connection;
- multiple loaded projects: enter a selection state and expose one action per
  loaded project.

For the browser-worker simulator, the zero-loaded-project case auto-loads the
demo project. Real hardware remains conservative and requires explicit project
loading when nothing is running.

## Link Management UX

Blank-device provisioning and recovery are modeled as link-level UX actions
because they happen below the running server protocol:

- `Provision firmware` is offered when the connected link session supports
  `FlashFirmware` and Studio is not currently attached to a server.
- `Reset to blank` is offered when the connected link session supports
  `EraseDeviceFlash`. It remains a tertiary destructive action even when the
  server is connected.

Both actions flow through `lpa-link::LinkProvider::manage_with_events`.
`StudioUx` clears project and server state before executing them because
firmware flashing and full-device erase invalidate any previous server/client
connection. Browser Web Serial ESP32 management streams esptool terminal output
and progress into the Link pane as `UxActivity` while the action is running.

After provisioning, Studio attempts to reopen the server protocol and resume the
normal server/project workflow. If the browser or device needs more time after
reset, Studio keeps the link context and reports that the user should reconnect
after boot.

For Browser Web Serial ESP32 links, opening or reopening the server protocol
goes through the provider reset path before Studio probes for loaded projects.
The browser-serial client waits for the first valid protocol frame before
sending the first request, so a just-reset device does not lose the initial
project probe while firmware is still booting.

After reset-to-blank, Studio leaves project and server detached and returns to a
link state that can offer provisioning again. Reset-to-blank is not a server
filesystem clear; it is a destructive whole-device erase through the link
provider.

Disconnect semantics are intentionally distinct:

- disconnecting a project detaches Studio from the project and leaves the server
  and link connected;
- disconnecting the server clears the project and server but leaves the link
  connected;
- disconnecting the link clears project/server/link and returns to provider
  selection.

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

## Removed Old Split

The old `lpa-studio-core` / `lpa-studio-runtime` split has been removed from
the active workspace. The UX crate owns the controller logic directly instead
of routing application work through a separate effect/event executor.

## Validation

```bash
cargo check -p lpa-studio-ux
cargo test -p lpa-studio-ux
cargo check -p lpa-studio-ux --target wasm32-unknown-unknown --features browser-worker,browser-serial-esp32
```
