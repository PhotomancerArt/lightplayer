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

## Providers

| Provider ID | Rust module/type | Runtime or device | Endpoint kind | Management intent | Status |
|---|---|---|---|---|---|
| `fake` | `providers::fake::FakeProvider` | none | test endpoint | diagnostics only | implemented |
| `local-host` | `providers::local_host::LocalHostProvider` | in-process `fw-host` | spawnable host runtime | logs, diagnostics, future local filesystem/runtime controls | implemented |
| `local-browser` | `providers::local_browser::LocalBrowserProvider` | `fw-browser` Web Worker | browser worker runtime | logs, diagnostics, worker lifecycle | model implemented; web code owns the actual Worker binding |
| `serial-esp32-web` | future `providers::serial_esp32_web::SerialEsp32WebProvider` | ESP32 over Web Serial | physical serial device | connect, reset, flash, raw filesystem, diagnostics | future |
| `serial-esp32-host` | future `providers::serial_esp32_host::SerialEsp32HostProvider` | ESP32 over host serial | physical serial device | connect, reset, flash, raw filesystem, diagnostics | future |
| `websocket` | future `providers::websocket::WebSocketProvider` | already-running server | remote endpoint | mostly connect/status; limited management | future |
| `webserver-host` | future `providers::webserver_host::WebserverHostProvider` | host service owning `fw-host` runtimes | service-managed runtime endpoint | create/stop runtimes, logs, diagnostics | future |

The ESP32 serial providers are intentionally ESP32-specific. Flashing,
resetting, boot-mode handling, and raw filesystem access are target-family
details; a generic serial abstraction can come later if another target earns it.

Provider support is feature-gated:

```bash
cargo check -p lpa-link
cargo test -p lpa-link
cargo check -p lpa-link --features local-host
cargo test -p lpa-link --features local-host
cargo check -p lpa-link --features local-browser --target wasm32-unknown-unknown
cargo test -p lpa-link --features local-browser
```

## Design Notes

- **Provider:** source of endpoints and management behavior, such as
  `local-host`, `local-browser`, or future ESP32 serial providers.
- **Endpoint:** something a provider can connect to. An endpoint can be physical
  hardware or a spawnable runtime target.
- **Session:** live ownership/lifecycle of a connected endpoint or launched
  runtime.
- **Connection:** client protocol channel to `lp-server`, consumed by
  `lpa-client`.
- **Management:** low-level operations below Studio capabilities: reset, flash,
  raw filesystem access, logs, diagnostics, and similar device/runtime controls.
- Public domain types use `Link*` names where they cross crate boundaries:
  `LinkProvider`, `LinkEndpoint`, `LinkSession`, `LinkConnection`, and related
  IDs/status types.
- Provider modules and methods use natural names such as `local_host`,
  `local_browser`, `discover`, `status`, `connect`, and `logs`.
- Public provider IDs use kebab-case, such as `local-host` and future
  `serial-esp32-web`. Rust modules/types use Rust naming, such as
  `providers::serial_esp32_web::SerialEsp32WebProvider`.
- The model is plural-first. Multiple host or browser runtime instances should
  be natural, even if the first Studio UI exposes only one session.
- `local-host` endpoints are spawnable. Calling `connect()` creates a new
  in-process `fw-host` runtime instance and returns a session that owns its
  lifecycle.
- A `LinkConnection` is a server/client connection, not a project session.
  Project sessions belong above this layer.
- `local-browser` is worker-shaped but not Rust-owned. The link layer can model
  endpoint/session identity, status, logs, diagnostics, and the worker envelope
  protocol. The web frontend must still bind that model to an actual module
  Worker created from `fw-browser/www/fw-browser-worker.js`.
