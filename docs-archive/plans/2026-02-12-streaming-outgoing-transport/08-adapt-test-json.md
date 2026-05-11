# Phase 8: Adapt test_json

## Scope of phase

Adapt test_json to use the new transport architecture: Channel<ServerMessage, 1>, io_task serializes. test_json validates ser-write-json on ESP32 hardware without the full server.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions at the bottom of files
- Keep related functionality grouped together
- Any temporary code should be a TODO comment so we find it later

## Implementation Details

### 1. Restore test_json

If removed in phase 1, recreate. test_json uses:
- Minimal setup: board init, io_task, MessageRouter
- Sends Heartbeat ServerMessage every second via the server message channel
- io_task receives and serializes (already implemented in phase 3)
- Blinks LED for visual feedback

### 2. test_json flow

- Create OUTGOING_SERVER_MSG (or use from io_task - need to share)
- io_task must be spawned with the channels; test_json pushes ServerMessage to OUTGOING_SERVER_MSG
- io_task receives, serializes, writes M!-prefixed JSON to serial

### 3. Shared channels

test_json needs to send to OUTGOING_SERVER_MSG. The io_task's `get_server_msg_channel()` returns the sender side (or we use the channel directly). Main test loop: `get_server_msg_channel().send(msg).await` or `try_send` if we want non-blocking. With capacity 1, send will block if io_task hasn't received - use `.await` for proper backpressure.

### 4. Feature gate

Ensure test_json feature enables server, lp-model/ser-write-json. justfile fwtest-json-esp32c6 recipe (restore if reverted in phase 1).

## Validate

```bash
just build-fw-esp32
just fwtest-json-esp32c6
```

Expect: Firmware builds with test_json feature. Flashing and running shows M! Heartbeat JSON on serial every second.
