# lp-studio-runtime

`lp-studio-runtime` executes `lp-studio-core` effects and turns lower-level
runtime/link/client facts back into Studio events.

## Boundaries

- `lp-studio-core` owns state transitions.
- `lp-studio-runtime` owns I/O, runtime adapters, demo project seeding, and
  client protocol flow.
- `lp-studio-web` owns Dioxus components and browser presentation.

The host-process path is:

```text
StudioEffect -> lpa-link host-process -> fw-host -> lpc-wire protocol
```

The browser-worker path is:

```text
StudioEffect -> lpa-link browser-worker model -> JavaScript Worker -> fw-browser
```

The browser serial ESP32 path is:

```text
StudioEffect -> lpa-link browser-serial-esp32 model -> Web Serial shim -> ESP32 lp-server
```

Demo project loading uses the same server protocol on both paths: write files
under `/projects/studio-demo/...`, then call `LoadProject` with
`studio-demo`.

The demo upload request list lives in `demo_project`, so future hardware paths
such as `browser-serial-esp32` can reuse the same `lp-server` filesystem writes
instead of forking project sync behavior. Direct/raw filesystem image access is
not part of this server protocol path; it belongs below the client connection in
`lpa-link` management.

`browser-serial-esp32` targets an already-flashed ESP32 running LightPlayer. It
uses a small JavaScript shim because `web-sys` currently gates Web Serial behind
unstable API cfg flags; Rust still owns Studio state, request/response handling,
and project upload semantics.

For hardware bring-up, valid `M!` protocol frames stay internal to the runtime.
Non-protocol device lines are echoed directly to the JavaScript console with a
`fw-esp32` prefix, using the firmware log level when present. They do not enter
the global Studio log list; a future hardware console view should live with the
device panel. Malformed `M!` frames are surfaced as warnings with a sanitized
JSON snippet so protocol/framing bugs can be diagnosed without dumping full
project payloads. If a malformed frame contains a nested `M!` marker, the
browser serial client attempts to resynchronize from that marker so a valid
response frame is not lost behind a truncated heartbeat or log burst.

## Validation

```bash
cargo check -p lp-studio-runtime --features host-process
cargo test -p lp-studio-runtime --features host-process
cargo check -p lp-studio-runtime --target wasm32-unknown-unknown --features browser-worker
cargo check -p lp-studio-runtime --target wasm32-unknown-unknown --features browser-worker,browser-serial-esp32
```
