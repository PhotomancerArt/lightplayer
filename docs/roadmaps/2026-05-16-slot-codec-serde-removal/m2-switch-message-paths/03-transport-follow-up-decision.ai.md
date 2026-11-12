# Phase 3: Decide Transport Fallback Handling

## Scope Of Phase

In scope:

- inspect `lpc-shared::transport::ServerTransport::send_project_read`
- decide whether the desktop/default fallback can send project-read JSON without
  deserializing it into `ProjectReadResponse`
- implement a small hook only if it stays contained
- otherwise document the follow-up in the roadmap or a local TODO note

Out of scope:

- broad transport redesign
- changing all client transports
- removing serde from all server messages

## Code Organization Reminders

- Prefer a tiny trait method over a broad transport refactor.
- Keep default behavior working for local/desktop transports.
- Do not make ESP32 streaming worse.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-core/lpc-shared/src/transport/server.rs`
- `lp-fw/fw-esp32/src/transport.rs`
- `lp-app/lpa-server/src/server.rs`
- `lp-app/lpa-client/src/local.rs`
- `lp-fw/fw-core/src/transport/serial.rs`

Current default:

```rust
let bytes = source.write_project_read_json(request, Vec::new())?;
let response: ProjectReadResponse = lpc_wire::json::from_slice(&bytes)?;
self.send(WireServerMessage { ... response }).await
```

This deserializes SlotCodec-written slot payloads back through Serde. If a small
hook can avoid that, add it. For example, a default method that accepts already
framed server-message JSON may be enough for transports that naturally send
bytes/strings.

If the change spreads across too many transports, do not implement it in M2.
Instead, add a follow-up note to the roadmap explaining that the ESP32 path is
already streaming, while default desktop fallback still round-trips through
serde until a later transport cleanup.

## Validate

Choose validation based on the change:

```bash
cargo test -p lpc-shared
cargo test -p lpa-server --no-run
cargo test -p lpc-engine project_read_stream
```

If no code change is made, run:

```bash
cargo test -p lpc-engine project_read_stream
```
