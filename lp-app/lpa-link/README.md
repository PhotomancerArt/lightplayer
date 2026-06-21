# lpa-link

`lpa-link` provides the mechanism by which an application like Studio or the CLI
connects to an `lp-server`.

Link providers allow discovery and management of their transports and underlying
hardware. A serial link for ESP32 provides firmware flashing, resetting, raw
filesystem access, diagnostics, and a client connection to the running
LightPlayer server.

This crate sits below Studio capabilities and beside `lpa-client`. A link
provider owns discovery, endpoint identity, endpoint status, low-level
management, raw logs, diagnostics, and opening a server/client connection. Once
a connection exists, `lpa-client` remains the typed client API for talking to
`lp-server`.

## Why This Is Not Just Transport

Real LightPlayer links need more than `connect()`. Depending on the provider,
the same low-level surface may need to discover ports or workers, report what is
connected, reset a device, flash firmware, inspect raw filesystem state, read
diagnostics, stream logs, and then open a server connection.

Studio should build product capabilities above this crate. It should not embed
Web Serial, browser-worker, host-process, flashing, or endpoint-management
details directly in UI code.

## Endpoint, Session, Connection

The central lifecycle is:

```text
LinkProvider::discover() -> LinkEndpoint
LinkProvider::connect(endpoint_id) -> LinkSession
LinkSession::connection() -> LinkConnection
LinkConnection::server_client() -> lpa-client
```

- **Endpoint:** a provider-visible target that can be opened. It is a
  discoverable candidate, not a live resource. Examples include an ESP32 serial
  port, a browser worker runtime target, a spawnable `fw-host` runtime, or a
  future websocket target.
- **Session:** live ownership of an opened endpoint. It owns provider-specific
  lifecycle, such as an open serial port, a spawned `fw-host` runtime, or a
  browser worker identity.
- **Connection:** the handoff from a live session to the `lp-server` protocol
  layer. A connection is not a project session and does not replace the owning
  link session.

## Server Connections

`LinkConnection` is the handoff point from an open link session to the server
protocol. Host providers currently expose a `LinkServerConnection`, which is a
shared host `lpa-client` transport and can be wrapped as a `TokioLpClient` with
`server_client()`.

Browser providers model the endpoint/session/protocol identity, but the actual
browser stream binding still lives in the web runtime:

- `browser-worker` binds a JavaScript module Worker created from `fw-browser`.
- `browser-serial-esp32` binds a Web Serial stream granted by a user gesture.

Those browser bindings should adapt their send/receive streams into
`lpa-client::ClientIo` rather than reimplement request ids, response
correlation, server error handling, heartbeat/log handling, or project deploy
ordering.

## Providers

| Provider ID | Rust module/type | Runtime or device | Endpoint kind | Management intent | Status |
|---|---|---|---|---|---|
| `fake` | `providers::fake::FakeProvider` | none | test endpoint | diagnostics only | implemented |
| `host-process` | `providers::host_process::HostProcessProvider` | host process running `fw-host` | spawnable host runtime | logs, diagnostics, future local filesystem/runtime controls | implemented; returns host `LinkServerConnection` |
| `browser-worker` | `providers::browser_worker::BrowserWorkerProvider` | `fw-browser` Web Worker | browser worker runtime | logs, diagnostics, worker lifecycle | model implemented; web code owns Worker binding and future `ClientIo` adapter |
| `host-serial-esp32` | `providers::host_serial_esp32::HostSerialEsp32Provider` | ESP32 over host serial | physical serial device | connect, reset-after-open, logs, diagnostics; future flash/raw filesystem | implemented for discovery/connect; returns host `LinkServerConnection` |
| `browser-serial-esp32` | `providers::browser_serial_esp32::BrowserSerialEsp32Provider` | ESP32 over Web Serial | physical serial device | connect, flash, reset, logs, diagnostics; future raw filesystem | model implemented; web code owns Web Serial binding, flashing adapter, and `ClientIo` adapter |
| `host-websocket` | future `providers::host_websocket::HostWebsocketProvider` | already-running server over host networking | remote endpoint | host-side discovery/connect/status; limited management | future |
| `browser-websocket` | future `providers::browser_websocket::BrowserWebsocketProvider` | already-running server over browser networking | remote endpoint | browser permission/discovery/connect/status; limited management | future |
| `host-webserver` | future `providers::host_webserver::HostWebserverProvider` | host service owning `fw-host` runtimes | service-managed runtime endpoint | create/stop runtimes, logs, diagnostics | future |

The ESP32 serial providers are intentionally ESP32-specific. Flashing,
resetting, boot-mode handling, and raw filesystem access are target-family
details; a generic serial abstraction can come later if another target earns it.

Provider support is feature-gated:

```bash
cargo check -p lpa-link
cargo test -p lpa-link
cargo check -p lpa-link --features host-process
cargo test -p lpa-link --features host-process
cargo check -p lpa-link --features host-serial-esp32
cargo test -p lpa-link --features host-serial-esp32
cargo check -p lpa-link --features browser-serial-esp32
cargo test -p lpa-link --features browser-serial-esp32
cargo check -p lpa-link --features browser-serial-esp32 --target wasm32-unknown-unknown
cargo check -p lpa-link --features browser-worker --target wasm32-unknown-unknown
cargo test -p lpa-link --features browser-worker
```

## Design Notes

- **Provider:** source of endpoints and management behavior, such as
  `host-process`, `browser-worker`, or ESP32 serial providers.
- **Endpoint:** discoverable candidate target. It has identity, status, and
  `LinkCapabilities`, but no live resource ownership.
- **Session:** live ownership/lifecycle of a connected endpoint or launched
  runtime.
- **Connection:** server protocol handoff to `lp-server`, consumed by
  `lpa-client`.
- **Capabilities:** low-level operations below Studio product actions: reset,
  flash, raw filesystem image access, logs, diagnostics, and similar
  device/runtime controls.
- Public domain types use `Link*` names where they cross crate boundaries:
  `LinkProvider`, `LinkEndpoint`, `LinkSession`, `LinkConnection`, and related
  IDs/status types.
- Provider modules and methods use natural names such as `host_process`,
  `browser_worker`, `discover`, `status`, `connect`, and `logs`.
- Public provider IDs use kebab-case and generally follow
  `{environment}-{mechanism}-{target?}`, such as `host-process`,
  `browser-worker`, `host-serial-esp32`, `browser-serial-esp32`,
  `host-websocket`, and `browser-websocket`. The target segment is optional when
  the mechanism already carries the whole contract. Include it when management
  details are target-specific. Rust modules/types use Rust naming, such as
  `providers::host_serial_esp32::HostSerialEsp32Provider`.
- The model is plural-first. Multiple host or browser runtime instances should
  be natural, even if the first Studio UI exposes only one session.
- `host-process` endpoints are spawnable. Calling `connect()` creates a new
  in-process `fw-host` runtime instance and returns a session that owns its
  lifecycle.
- A `LinkConnection` is a server/client connection, not a project session.
  Project sessions belong above this layer.
- `browser-worker` is worker-shaped but not Rust-owned. The link layer can model
  endpoint/session identity, status, logs, diagnostics, and the worker envelope
  protocol. The web frontend must still bind that model to an actual module
  Worker created from `fw-browser/www/fw-browser-worker.js`.
- `browser-serial-esp32` is Web-Serial-shaped but not Rust-owned by `lpa-link`.
  The link layer models granted endpoints, sessions, management capability, and
  the serial JSON-lines protocol identity. The web runtime calls `requestPort()`
  from a user gesture and binds the browser streams to protocol read/write
  logic. Firmware flashing is also browser-owned: Studio releases the normal
  protocol reader/writer, then the browser flashing adapter takes exclusive
  ownership of the same granted `SerialPort` for bootloader flashing.
- Direct filesystem access means raw/full filesystem image management below the
  running `lp-server`. Normal project upload should use `lpa-client` and the
  server filesystem/project protocol once firmware is running.
