# lpa-link

`lpa-link` is the low-level app-side link layer for LightPlayer endpoints.

It sits below Studio capabilities and beside `lpa-client`. A link provider owns
discovery, endpoint identity, endpoint status, low-level management, raw logs,
diagnostics, and opening a server/client connection. Once a connection exists,
`lpa-client` remains the typed client API for talking to `lp-server`.

## Why This Is Not Just Transport

Real LightPlayer links need more than `connect()`. Depending on the provider,
the same low-level surface may need to discover ports or workers, report what is
connected, reset a device, flash firmware, inspect raw filesystem state, read
diagnostics, stream logs, and then open a server connection.

Studio should build product capabilities above this crate. It should not embed
Web Serial, browser-worker, host-process, flashing, or endpoint-management
details directly in UI code.

## Providers

- `providers::fake` is a deterministic test provider and future Studio-core
  harness.
- `providers::local_host` launches host-local runtime instances through
  `fw-host` and returns a connection usable by `lpa-client`.
- `providers::local_browser` models browser/Web Worker runtime instances for
  Studio simulation and project testing. Its connection kind records the
  `fw-browser-post-message-v1` envelope; Studio web code owns the actual
  JavaScript `Worker` object and postMessage transport binding.

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

- Public domain types use `Link*` names where they cross crate boundaries:
  `LinkProvider`, `LinkEndpoint`, `LinkSession`, `LinkConnection`, and related
  IDs/status types.
- Provider modules and methods use natural names such as `local_host`,
  `local_browser`, `discover`, `status`, `connect`, and `logs`.
- The model is plural-first. Multiple host or browser runtime instances should
  be natural, even if the first Studio UI exposes only one session.
- A `LinkConnection` is a server/client connection, not a project session.
  Project sessions belong above this layer.
- `local-browser` is worker-shaped but not Rust-owned. The link layer can model
  endpoint/session identity, status, logs, diagnostics, and the worker envelope
  protocol. The web frontend must still bind that model to an actual module
  Worker created from `fw-browser/www/fw-browser-worker.js`.
