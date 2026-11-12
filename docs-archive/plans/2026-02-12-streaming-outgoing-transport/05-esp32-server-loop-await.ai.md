# Phase 5: ESP32 Server Loop Verification

## Scope of phase

Verify fw-esp32 server_loop correctly awaits `transport.send()` and `transport.receive()`. Phase 2 added `.await`; phase 4 switched to StreamingMessageRouterTransport. Ensure heartbeat and response sends use the new async transport correctly.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions at the bottom of files
- Keep related functionality grouped together
- Any temporary code should be a TODO comment so we find it later

## Implementation Details

### 1. Verify server_loop.rs

- `transport.receive().await?` in the receive loop
- `transport.send(server_msg).await?` when sending responses
- `transport.send(heartbeat_msg).await` for heartbeat

### 2. Handle receive_all if used

If server_loop uses `receive_all()`, ensure it's `transport.receive_all().await?`.

## Validate

```bash
just build-fw-esp32
cargo test -p fw-esp32 --no-run
```

Expect: Build succeeds. Manual device test: run firmware, verify M! messages appear on serial.
