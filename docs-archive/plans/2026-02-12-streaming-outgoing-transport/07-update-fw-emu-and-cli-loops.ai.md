# Phase 7: Update fw-emu and CLI Server Loops

## Scope of phase

Ensure fw-emu and CLI server loops correctly integrate with async ServerTransport. fw-emu uses block_on for sync context; CLI may already be async.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions at the bottom of files
- Keep related functionality grouped together
- Any temporary code should be a TODO comment so we find it later

## Implementation Details

### 1. fw-emu server_loop

- Runs in RISC-V guest, sync context
- Use appropriate block_on: `embassy_futures::block_on(transport.send(msg))` or equivalent
- Check if fw-emu has an executor - it may use a different runtime
- Per 2026-02-04 plan: block_on is safe in sync context

### 2. CLI server loops

- **run_server_loop_async.rs**: Already async; add `.await` to transport calls
- **serve/server_loop.rs** (sync): Use `runtime.block_on()` or `tokio::task::block_in_place` to call async transport

### 3. Tests

Update any integration tests that spawn server loops.

## Validate

```bash
just build-app
just build-fw-esp32
cargo test -p fw-emu
cargo test -p lp-cli
```

Expect: fw-emu and lp-cli build; tests pass.
