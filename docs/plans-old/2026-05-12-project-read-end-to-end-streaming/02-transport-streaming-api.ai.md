# Phase 2: Transport Streaming API

## Scope Of Phase

Extend the server transport boundary so large responses can be streamed without
first constructing a full `WireServerMessage`.

In scope:

- Add a streaming-capable API to `ServerTransport` or a closely related helper
  trait.
- Provide simple fallback behavior for desktop/test transports.
- Add or update tests for transport implementations where practical.

Out of scope:

- Rewriting `LpServer::tick`.
- ESP serial writer internals.
- Engine result-level streaming improvements.

## Code Organization Reminders

- Prefer granular files with one main concept per file.
- Keep the transport trait small and semantic.
- Avoid making low-level shared crates depend on high-level engine crates unless
  there is no cleaner boundary.
- Put helpers lower in files when that improves readability.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-core/lpc-shared/src/transport/server.rs`
- `lp-app/lpa-client/src/local.rs`
- `lp-fw/fw-core/src/transport/message_router.rs`
- `lp-fw/fw-core/src/transport/fake.rs`
- `lp-fw/fw-esp32/src/transport.rs`
- `lp-cli/src/server/transport_ws.rs`

Expected changes:

- Add a streaming response hook that can write a large response through a
  `lpc-wire` direct writer.
- Prefer a generic writer/callback boundary over adding an `lpc-engine`
  dependency to `lpc-shared`.
- Desktop/test transports may implement the hook by collecting direct-written
  bytes into a `Vec<u8>` and then sending/parsing normally.
- Firmware transports should expose a true bounded streaming path in later
  phases.

Possible shape:

```rust
async fn send_project_read_stream(
    &mut self,
    id: u64,
    write: impl FnOnce(/* transport JsonWrite sink */) -> Result<(), _>,
) -> Result<(), TransportError>;
```

The exact signature should be chosen for Rust ergonomics, object safety, and
current call sites. If async trait generics get ugly, use an associated adapter
or a small enum/request object and document the tradeoff.

Constraints:

- Keep normal `send(WireServerMessage)` for small messages.
- Do not require desktop transports to become low-memory streamers.
- Do not hand-write JSON in each transport; direct writers should live in
  `lpc-wire`.

## Validate

```bash
cargo fmt --check
cargo check -p lpc-shared
cargo test -p lpc-wire
cargo test -p lpa-client
cargo check -p fw-core
```

