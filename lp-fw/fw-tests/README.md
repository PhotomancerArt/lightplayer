# Firmware Tests

Integration tests for firmware functionality, including firmware emulator
rendering and USB serial communication tests.

## Firmware Emulator Render Tests

`scene_render_emu` builds and runs `fw-emu`, writes a project through the wire
protocol, loads it, advances simulated time, and verifies output channel bytes
through canonical project-read `OutputChannels` resources.

```bash
cargo test -p fw-tests --test scene_render_emu
```

Browser firmware smoke coverage currently lives with `fw-browser`: the
`fw-browser` wasm test covers project load/tick/output through the runtime API,
and `lp-fw/fw-browser/www/smoke.html` creates a real Web Worker and verifies the
same project-read output path through `postMessage`. A future CI browser runner
can move or mirror the Web Worker smoke here if that becomes easier to maintain.

## USB Serial Tests

Automated integration tests for ESP32 USB serial communication, verifying that the firmware:
- Starts correctly without a serial connection
- Handles serial connection/disconnection gracefully
- Continues operating (LED blinking, frame counting) when serial is disconnected
- Successfully reconnects after disconnection

### Prerequisites

- ESP32-C6 device connected via USB
- `cargo-espflash` installed: `cargo install cargo-espflash`
- Only one ESP32 device connected (tests look for `/dev/cu.usbmodem*` on macOS)

### Running the Tests

```bash
# Run all USB serial tests
cargo test --package fw-tests --features test_usb -- --ignored

# Run a specific test
cargo test --package fw-tests --features test_usb -- --ignored test_scenario_1_start_without_serial

# Run with debug output (shows detailed command output)
DEBUG=1 cargo test --package fw-tests --features test_usb -- --ignored
```

### Test Output

The tests provide clean, professional output with:
- ✓/✗ status indicators
- Elapsed time for each operation
- Technical details (port names, values) shown in dim/grey
- Command/response details only when `DEBUG=1` is set

### Test Scenarios

1. **Start without serial**: Firmware starts, waits, then connects serial and verifies operation
2. **Start with serial**: Firmware starts with serial already connected
3. **Echo and reconnect**: Tests echo command functionality and reconnection

### Manual Testing

You can also connect manually with `screen` or similar tools to see the heartbeat messages:

```bash
screen /dev/cu.usbmodem* 115200
```

You should see heartbeat messages every second:
```
heartbeat: frame_count=12345
heartbeat: frame_count=23456
...
```

Tests automatically filter out heartbeat messages, so they don't interfere with test responses.
