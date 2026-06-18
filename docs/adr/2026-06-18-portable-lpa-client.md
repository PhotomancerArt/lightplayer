# ADR: Portable lpa-client And Link Server Connections

- **Status:** Accepted
- **Date:** 2026-06-18
- **Deciders:** Photomancer
- **Supersedes:** None
- **Superseded by:** None

## Context

LightPlayer now has multiple app-side ways to reach a running `lp-server`:
host-process, host serial ESP32, browser worker, browser serial ESP32, and
future websocket/server-owned variants. The CLI grew first and its `LpClient`
was shaped around Tokio, shared mutexes, request timeouts, host transports, and
CLI heartbeat rendering.

Studio web is now the primary app direction. It needs the same server protocol
semantics in browser runtimes and future agent harnesses without forcing Tokio,
`Send`, or host transport types into the core client model. At the same time,
the CLI and native host paths should keep their current ergonomics.

`lpa-link` also needs a crisp boundary. It owns endpoint discovery, status,
logs, diagnostics, reset, flashing, raw filesystem access, and connection
lifecycle. It should not grow typed project operations. Once a link reaches a
running server, `lpa-client` should own the server protocol.

## Decision

Make `lpa-client::LpClient<Io>` the portable, runtime-neutral client for the
LightPlayer server protocol.

- `ClientIo` is the small portable send/receive/close contract over
  `lpc-wire` messages. It does not require Tokio or `Send`.
- `LpClient<Io>` owns request id allocation, response correlation, server error
  handling, typed filesystem/project/overlay operations, and protocol events.
- Heartbeats, server logs, and uncorrelated responses are returned as
  `ClientEvent`s instead of being printed or mapped to UI state in the core.
- Project deploy ordering lives in `lpa-client` helpers so Studio, CLI, and
  agents can share stop/write/load semantics.
- Host/native behavior lives in `TokioLpClient` and host transport modules:
  shared Tokio mutexes, timeouts, current CLI heartbeat display, local
  transports, serial transports, and websocket transports.
- Host support remains enabled by default for existing native callers, while
  browser/core consumers can compile with `default-features = false`.
- `lpa-link` exposes server connections from connected sessions. Host links
  return a `LinkServerConnection` that can become a `TokioLpClient`; browser
  links model endpoint/session/protocol identity and should adapt their browser
  streams into `ClientIo`.
- The dependency direction is `lpa-link -> lpa-client` only for connection
  types/adapters. `lpa-client` must not depend on `lpa-link`.

## Consequences

Studio browser work can reuse server protocol semantics without copying request
id, response correlation, heartbeat/log, server error, or project deploy logic.

The CLI keeps a cloneable Tokio client wrapper and current command behavior,
but that wrapper is now a host adapter rather than the definition of the client
model.

`lpa-link` stays a low-level device/runtime link layer. It can grow ESP32
flashing, reset, raw filesystem, and diagnostics without confusing those
management operations with typed project/client protocol operations.

There are temporarily two browser-local protocol clients in
`lp-studio-runtime` for M2 hardware bring-up. They are now explicitly follow-up
work: M2c should adapt browser worker and Web Serial streams into `ClientIo`.

The feature model is slightly more explicit. Host adapters and host transports
sit behind `host`; the portable core can be checked for wasm with
`cargo check -p lpa-client --target wasm32-unknown-unknown --no-default-features`.

## Alternatives Considered

- Keep the Tokio `LpClient` as the core model.
  - Rejected because it would make Studio web and agent harnesses inherit host
    runtime choices.
- Split out a new `lpa-client-core` crate immediately.
  - Rejected because the dependency cycle did not require it; feature-gated
    modules keep the boundary clear with less crate churn.
- Keep browser serial and worker protocol logic inside `lp-studio-runtime`.
  - Rejected as the final architecture because it would duplicate foundational
    protocol semantics. Accepted only as temporary M2 bring-up code until M2c
    cuts browser streams over to `ClientIo`.
- Put typed project operations in `lpa-link`.
  - Rejected because links should own discovery/management/connection, while
    `lpa-client` owns server protocol semantics once connected.

## Follow-ups

- M2c: adapt browser Web Serial to `lpa-client::ClientIo` and remove duplicated
  response-correlation logic from the browser serial runtime path.
- Adapt browser worker protocol handling to `ClientIo` when the Studio worker
  path needs richer project operations.
- Consider moving host serial framing/resynchronization helpers below
  `ClientTransport` if host and browser serial hardening converge.
- Keep ESP32 multi-project behavior conservative by using stop/write/load
  deploy helpers until firmware intentionally supports multiple live projects.
