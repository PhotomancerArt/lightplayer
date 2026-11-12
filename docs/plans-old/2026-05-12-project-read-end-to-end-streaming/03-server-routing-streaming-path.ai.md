# Phase 3: Server Routing Streaming Path

## Scope Of Phase

Route project-read requests through the streaming transport path so
`lpa-server` does not construct a full `ProjectReadResponse` for firmware
debug reads.

In scope:

- Add a streaming-aware server loop entry point or server method.
- Route `ClientRequest::ProjectRequest { request: WireProjectRequest::Read }`
  through `Engine::write_project_read_json`.
- Keep normal responses using existing `WireServerMessage`.
- Preserve current host behavior where reasonable.
- Add tests for semantic equivalence between streamed project reads and
  `Engine::read_project`.

Out of scope:

- Deep streaming of individual engine query results.
- ESP serial sink optimization.
- Rebuilding debug UI.

## Code Organization Reminders

- Prefer granular files with one main concept per file.
- Keep normal message handlers readable; do not bury project-read routing in a
  giant match arm if a helper module makes it clearer.
- Put helpers lower in files when that improves readability.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-app/lpa-server/src/server.rs`
- `lp-app/lpa-server/src/handlers.rs`
- `lp-app/lpa-server/src/project_manager.rs`
- `lp-app/lpa-server/src/project.rs`
- `lp-cli/src/server/run_server_loop_async.rs`
- `lp-fw/fw-esp32/src/server_loop.rs`
- `lp-fw/fw-emu/src/server_loop.rs`
- `lp-core/lpc-engine/src/engine/project_read_stream.rs`

Expected changes:

- Add a path that processes incoming client messages and can send responses
  directly through a transport.
- Keep `tick(...) -> Vec<WireMessage>` for tests/simple callers if useful, but
  firmware should use the streaming-aware path.
- For project read:
  - validate the project handle,
  - borrow the project engine,
  - call the transport streaming method with the engine writer,
  - avoid storing a borrowed engine in a response object.
- For errors and all non-project-read messages, keep normal response messages.
- Host async loop can use the same server streaming path; its transport may
  collect into memory internally.

Borrowing concern:

- Do not create a `ServerOutput` that owns references across frames.
- Prefer local borrow during the awaited transport streaming call.
- If Rust borrow rules make this difficult, split ticking and message handling
  so the project borrow is not overlapping unrelated server borrows.

Tests:

- Add a host test that sends a `ProjectReadRequest::default_debug(None)` through
  the streaming server path and deserializes the client-visible response.
- Compare to `Engine::read_project` where practical.

## Validate

```bash
cargo fmt --check
cargo test -p lpa-server
cargo test -p lpc-engine streaming_project_read_matches_full_debug_response
cargo check -p lp-cli
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server
```

