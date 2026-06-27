# lpa-client

`lpa-client` owns the typed client protocol for talking to a running
LightPlayer `lp-server`.

The crate is split into two layers:

- `LpClient<Io>` is the portable protocol client. It owns request ids,
  response correlation, server errors, heartbeats/log events, and typed
  filesystem/project/overlay operations. It depends on a small `ClientIo` trait
  and does not require Tokio or `Send`.
- Host adapters provide the current native ergonomics: cloneable shared
  transports, Tokio timeouts, serial/websocket/local transports, and CLI-style
  heartbeat/log rendering.

This keeps Studio web, CLI, host runtimes, and future agents on one protocol
model while allowing each runtime to bind its own I/O.

Project reads are multi-frame operations: one `ProjectReadRequest` can produce
several same-id `ProjectReadFrame` messages before a terminal event. The public
client API still returns `ProjectReadResponse` by collecting those frames.

## Feature Model

| Feature | Purpose |
|---|---|
| `default` | Enables `host` for existing native callers. |
| `host` | Tokio/shared transport adapter, local in-memory transport, host specifier parsing, logging, and `TokioLpClient`. |
| `serial` | Host serial transport for ESP32/emulator-style JSON-lines links. Implies `host`. |
| `emu` | Emulator serial transport support. Implies `host`. |
| `ws` | Host websocket transport. Implies `host`. |

Portable/browser-oriented consumers should depend on the core without defaults:

```toml
lpa-client = { path = "../lpa-client", default-features = false }
```

The core compile check is:

```bash
cargo check -p lpa-client --target wasm32-unknown-unknown --no-default-features
```

## Important Types

- `ClientIo`: runtime-neutral send/receive/close trait for `lpc-wire` messages.
- `LpClient<Io>`: typed protocol client over any `ClientIo`.
- `ClientOutcome<T>`: operation result plus protocol events observed while
  waiting for the correlated response.
- `ClientEvent`: heartbeat/log/uncorrelated-response events surfaced to the
  caller.
- `ProjectDeployFile`: one project file for shared stop/write/load deploy
  helpers.
- `TokioLpClient`: host wrapper that preserves the CLI/native shared-client API.
- `ClientTransport`: host-only Tokio transport trait used by native providers.

## Project Deploy Semantics

Server-protocol project deploys should use this crate rather than open-coding
request sequences. The shared deploy flow is currently:

1. `StopAllProjects`
2. write files under `/projects/{project_id}/...`
3. `LoadProject { path: "projects/{project_id}" }`

That ordering avoids the ESP32 trying to run multiple loaded projects during a
replace-in-place upload. Direct bootloader/raw filesystem image access is not a
server-protocol deploy; it belongs below this layer in `lpa-link` management.

Use `deploy_project_files` for initial upload/load flows such as CLI upload,
CLI dev startup, firmware demo checks, and browser hardware demo loading. Use
`push_project_files` only when the caller intentionally wants write-only sync,
such as an already-loaded file-watch update.

## Relationship To lpa-link

`lpa-link` owns device/runtime discovery, endpoint status, raw logs,
diagnostics, reset, flashing, and raw filesystem access. When a link is
connected to a running `lp-server`, it exposes a server connection that callers
can wrap with this crate.

Keep server protocol semantics here. Keep low-level device management in
`lpa-link`.

## Validation

```bash
cargo check -p lpa-client
cargo test -p lpa-client
cargo check -p lpa-client --target wasm32-unknown-unknown --no-default-features
```
