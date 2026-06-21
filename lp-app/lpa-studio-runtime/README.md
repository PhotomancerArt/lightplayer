# lpa-studio-runtime

`lpa-studio-runtime` executes `lpa-studio-core` effects and turns lower-level
runtime/link/client facts back into Studio events.

## Boundaries

- `lpa-studio-core` owns state transitions.
- `lpa-studio-runtime` owns effect execution, runtime adapters, demo project
  seeding, and mapping lower-level client/link events into Studio events.
- `lpa-link` owns provider resource lifecycles: endpoint/session state, browser
  worker creation, Web Serial open/release/close, and ESP32 probe/flash.
- `lpa-client` owns lp-server request ids, response correlation, protocol
  errors, heartbeat/log events, and shared project deploy semantics.
- `lpa-studio-web` owns Dioxus components and browser presentation.

The host-process path is:

```text
StudioEffect -> lpa-link host-process -> lpa-client TokioLpClient -> fw-host
```

The browser-worker path is:

```text
StudioEffect -> lpa-link browser-worker provider -> JavaScript Worker -> fw-browser -> lp-server protocol
```

The browser serial ESP32 path is:

```text
StudioEffect -> lpa-link browser-serial-esp32 provider -> lpa-client LpClient<ClientIo> -> Web Serial -> ESP32 lp-server
```

The scenario path is I/O-free:

```text
StudioEffect -> scenario runtime -> scripted StudioEvent values -> StudioApp reducer
```

`scenario` is a runtime test and future story-fixture layer. It models
product-level provisioning outcomes such as permission denial, endpoint open
failure, blank devices, flash failure, project load failure, heartbeat, and
connection loss. It also models post-server project state: existing project,
no loaded project, multiple loaded projects, and recovery-required branches. It
does not replace `lpa-link` fake providers, which remain useful for lower-level
link/session behavior. Scenario tests drive the same action/effect/event/reducer
loop as real runtimes, so the UI and future agents can reuse the same vocabulary
without inventing separate fixture states.

Connected runtimes handle `ReadProjectState` by listing loaded projects through
the server protocol. Studio treats one loaded project as attachable, zero or
many loaded projects as user selection states, and future safe-mode data as a
recovery state. Explicit starter/demo upload remains a separate user action.

Demo project loading uses the same server protocol on both paths: write files
under `/projects/studio-demo/...`, then load the `studio-demo` project. Hardware
deploy flows stop existing projects before writing/loading so ESP32-class
firmware does not keep old output resources open while the new project starts.

The demo upload request list currently lives in `demo_project` for the Studio
POC. Longer-lived project deploy semantics live in `lpa-client`, including the
shared stop/write/load flow that future hardware paths should use instead of
forking request correlation or project sync behavior. Direct/raw filesystem
image access is not part of this server protocol path; it belongs below the
client connection in `lpa-link` management.

`browser-serial-esp32` targets an already-flashed ESP32 running LightPlayer.
Web Serial ownership lives in `lpa-link`; the Rust runtime adapts provider
read/write operations into `lpa-client::ClientIo` so request correlation,
protocol events, server errors, and project write helpers come from the shared
client model.

The browser ESP32 flashing path is a provider operation in `lpa-link`. The
runtime advertises flash capability when provider checks succeed, requests probe
or flash by endpoint id, and translates low-level logs/progress/success/failure
into Studio events. Reconnect/classification after flash remains a separate
Studio provisioning step.

Browser serial target classification is layered. Studio first opens the normal
serial link and sends a lightweight `lp-server` request through `lpa-client`. If
that responds, the target is treated as a running LightPlayer server and Studio
continues to project-state discovery. If the server request times out or the
stream is not protocol-shaped, Studio releases the normal serial client and
uses the browser ESP32 adapter to detect a provisionable ESP32-C6 bootloader.
Unsupported or unresponsive targets become typed provisioning issues rather
than raw controller errors.

For hardware bring-up, valid `M!` protocol frames stay internal to the runtime.
Non-protocol device lines are echoed directly to the JavaScript console with a
`fw-esp32` prefix, using the firmware log level when present. They do not enter
the global Studio log list; a future hardware console view should live with the
device panel. Malformed `M!` frames are surfaced as warnings with a sanitized
JSON snippet so protocol/framing bugs can be diagnosed without dumping full
project payloads. If a malformed frame contains a nested `M!` marker, the
browser serial client attempts to resynchronize from that marker so a valid
response frame is not lost behind a truncated heartbeat or log burst.

The current ESP32 Studio deploy policy is single-project by workflow:

```text
StopAllProjects
FsWrite demo files
LoadProject
ReadProjectInventory
```

Future firmware/server capabilities can expose richer multi-project support,
but Studio hardware loading should keep this conservative flow until output
resource arbitration is designed.

## Validation

```bash
cargo check -p lpa-studio-runtime --features host-process
cargo test -p lpa-studio-runtime --features host-process
cargo check -p lpa-studio-runtime --target wasm32-unknown-unknown --features browser-worker
cargo check -p lpa-studio-runtime --target wasm32-unknown-unknown --features browser-worker,browser-serial-esp32
```
