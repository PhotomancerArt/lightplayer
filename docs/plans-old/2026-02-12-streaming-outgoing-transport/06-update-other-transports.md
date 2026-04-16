# Phase 6: Update Other Transports

## Scope of phase

Phase 2 already updated MessageRouterTransport, FakeTransport, SerialTransport, WebSocketServerTransport, and AsyncLocalServerTransport. This phase verifies all are correct and fixes any that were deferred or incorrect.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions at the bottom of files
- Keep related functionality grouped together
- Any temporary code should be a TODO comment so we find it later

## Implementation Details

### 1. Verify each transport

- **MessageRouterTransport**: Used by fw-core tests. async send serializes and try_send to router; receive drains incoming.
- **SerialTransport**: Used by fw-emu. async send writes to SerialIo.
- **FakeTransport**: Used in tests. Trivial async impl.
- **WebSocketServerTransport**: async send to mpsc. May need `block_on` if used from sync context.
- **AsyncLocalServerTransport**: async send to tokio mpsc.

### 2. fw-emu block_on

fw-emu server_loop runs in sync context. Use `embassy_futures::block_on` or the emulator's equivalent to call async transport. Verify fw-emu still builds and runs.

### 3. CLI server loops

Ensure both async and sync CLI server loops correctly await or block_on transport calls.

## Validate

```bash
just check
just build-fw-esp32
just build-app
cargo test -p fw-core -p lp-server -p lp-client -p lp-cli
```

Expect: All packages compile and tests pass.
