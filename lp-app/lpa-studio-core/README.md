# lpa-studio-core

`lpa-studio-core` owns the UI-independent LightPlayer Studio domain model.

It defines Studio state, documented actions, action metadata, effects, runtime
events, diagnostics, capabilities, provisioning state, and session records. UI
code, runtime code, tests, harnesses, and future agents should all speak through
this vocabulary.

## Boundaries

- This crate owns state transitions and effect descriptions.
- This crate does not perform I/O, spawn runtimes, talk to browser workers, open
  serial ports, or render UI.
- `lpa-studio-runtime` executes effects and emits events.
- `lpa-studio-web` renders state and dispatches actions.
- `lpa-link` remains the lower-level link/session/connection layer below Studio
  capabilities.

## Provisioning Model

Studio provisioning is modeled above `lpa-link`. The link layer knows how to
discover endpoints, open sessions, report low-level management capability, and
hand off a server connection. Studio core owns the product journey around that
link layer and exposes it as three manager read models.

| Manager | Owns | Ready means | Does not own |
|---|---|---|---|
| `link` | Provider catalog, provider availability, endpoint access, physical/logical link state, target probing, flashing/reset actions, low-level diagnostics | A target link is established and can support server work | `lp-server` protocol state or project meaning |
| `server` | `lp-server` protocol connection, status/heartbeat facts, logs, loaded-project discovery, recovery/safe-mode facts | The protocol is usable and current server status is known | Project editing/session state |
| `project` | Active project attachment, project inventory snapshot, project load/deploy/resync state, future sync/edit facts | Studio has a coherent project session, or knows user project selection is required | Link/device operations or raw server transport |

The managers are setup-sequential but not lifetime-sequential. The link manager
continues to own disconnect, reset, reinstall, and diagnostics actions after a
server or project is attached.

`DeviceManagerState`, `ServerState`, and `ProjectState` each expose
`available_actions()`. The combined `StudioState::available_actions()` returns
dispatchable `StudioActionKind` values for generic UI surfaces and future agent
harnesses.

Provider availability is intentionally separate from endpoint/device
capability. For example, "Web Serial is unsupported in this browser" is a
provider availability issue. "This ESP32 endpoint can flash firmware" is an
endpoint or management capability.

Actions are documented program objects:

- `LinkActionRequest`, `ServerActionRequest`, and `ProjectActionRequest` are
  manager-local dispatch payloads.
- `StudioActionKind` wraps those manager-local requests for app-wide dispatch.
- `ActionDescriptor` provides labels, summaries, categories, and history policy.
- `AvailableAction` combines a dispatchable payload with descriptor,
  enablement, priority, and optional confirmation metadata.
- `StudioEffect` describes work that a runtime must execute.
- `StudioEvent` is the runtime's result input back into the core state machine.

The hardware action surface separates:

- device access and browser permission requests;
- link/session operations such as connect, disconnect, reset, and flash;
- provisioning operations such as provider catalog refresh, target probing,
  progress updates, and typed issue recovery;
- project operations such as reading loaded project state, attaching to an
  existing loaded project, or explicitly uploading/loading the built-in starter
  project through `lp-server`;
- local navigation such as selecting a project node.

After a server link is established, Studio reads project state before loading
anything. A single loaded project becomes the attached `ProjectSession`; zero or
multiple loaded projects become `ProjectSelectionRequired`; safe/recovery
conditions become `RecoveryRequired`. This keeps normal connect-to-existing
hardware separate from explicit overwrite/load intent.

Operational hardware actions are not undoable. Future undo should attach to
successful project edit transactions, not to permission prompts, flashing,
resets, or connection lifecycle events.

This crate does not implement undo/redo yet. It only classifies action history behavior so
future undo can attach to successful project edit transactions instead of every
operational action.

## Validation

```bash
cargo check -p lpa-studio-core
cargo test -p lpa-studio-core
```
