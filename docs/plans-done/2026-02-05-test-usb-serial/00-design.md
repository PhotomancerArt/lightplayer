# Design: Test USB Serial Connection/Disconnection Scenarios

## Scope of Work

Create a comprehensive test suite for `fw-esp32` that verifies proper handling of USB serial connection/disconnection scenarios. The test structure should be production-ready and compatible with the main `lp-server` architecture.

## File Structure

```
lp-fw/
├── fw-core/
│   └── src/
│       ├── message_router.rs        # NEW: MessageRouter with embassy-sync channels
│       └── lib.rs                    # UPDATE: Export message_router
├── fw-esp32/
│   └── src/
│       ├── tests/
│       │   └── test_usb.rs          # REPLACE: New test_usb with MessageRouter
│       └── main.rs                   # UPDATE: Use new test_usb
└── fw-tests/
    └── tests/
        └── test_usb_serial.rs       # NEW: Automated host-side tests
```

## Conceptual Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    Main Loop Task                          │
│  ┌──────────────────────────────────────────────────────┐  │
│  │  loop {                                              │  │
│  │    blink_led()          // 2Hz blink (visual)      │  │
│  │    frame_count++        // Atomic counter           │  │
│  │    handle_messages()     // Process incoming queue   │  │
│  │  }                                                    │  │
│  └──────────────────────────────────────────────────────┘  │
└────────────────────┬────────────────────────────────────────┘
                     │ Uses
                     ▼
┌─────────────────────────────────────────────────────────────┐
│              MessageRouter (fw-core)                       │
│  ┌──────────────────────┐  ┌──────────────────────┐       │
│  │  Incoming Channel    │  │  Outgoing Channel    │       │
│  │  Channel<String, 32> │  │  Channel<String, 32> │       │
│  └──────────────────────┘  └──────────────────────┘       │
│         ▲                            │                     │
│         │                            ▼                     │
│         │              ┌──────────────────────┐            │
│         │              │  receive_all()      │            │
│         │              │  send(msg)          │            │
│         └──────────────│  (non-blocking)     │            │
│                        └──────────────────────┘            │
└────────────────────────────────────────────────────────────┘
         ▲                            │
         │                            │
         │                            ▼
┌─────────────────────────────────────────────────────────────┐
│              I/O Task (Serial Handler)                    │
│  ┌──────────────────────────────────────────────────────┐  │
│  │  loop {                                              │  │
│  │    // Drain outgoing queue, send via serial         │  │
│  │    while let Ok(msg) = outgoing.try_receive() {    │  │
│  │      serial_write(msg)                              │  │
│  │    }                                                 │  │
│  │                                                      │  │
│  │    // Read from serial, push to incoming queue      │  │
│  │    if let Some(line) = serial_read_line() {         │  │
│  │      if line.starts_with("M!") {                   │  │
│  │        incoming.try_send(line)                      │  │
│  │      }                                               │  │
│  │    }                                                 │  │
│  │                                                      │  │
│  │    // Handle serial state (Ready/Disconnected)      │  │
│  │    Timer::after(1ms).await                          │  │
│  │  }                                                   │  │
│  └──────────────────────────────────────────────────────┘  │
└────────────────────────────────────────────────────────────┘
```

## Main Components

### 1. MessageRouter (fw-core)

**Purpose**: Central message routing abstraction that decouples main loop from I/O.

**API**:
```rust
pub struct MessageRouter {
    incoming: &'static Channel<CriticalSectionRawMutex, String, 32>,
    outgoing: &'static Channel<CriticalSectionRawMutex, String, 32>,
}

impl MessageRouter {
    pub fn receive_all(&self) -> Vec<String>;
    pub fn send(&self, msg: String) -> Result<(), TrySendError<String>>;
}
```

**Features**:
- Uses `embassy-sync::channel::Channel` for queues
- Bounded queues (32 messages) with overflow handling
- Non-blocking operations (`try_receive`/`try_send`)
- Multi-transport ready (can extend to multiple routers)

### 2. Test Message Protocol

**Format**: `M!{...}\n` (JSON with prefix, newline-terminated)

**Commands** (external discriminators):
- `M!{"get_frame_count":{}}\n` - Query frame counter
- `M!{"echo":{"data":"test"}}\n` - Echo test message

**Responses**:
- `M!{"frame_count":12345}\n` - Frame count response
- `M!{"echo":"test"}\n` - Echo response

**Benefits**:
- Prefix `M!` filters out non-message data (debug prints)
- External discriminators efficient with serde-json-core
- Verb-based command names (`get_frame_count`)

### 3. Main Loop Structure

**Pattern**: `blink_led()` → `handle_messages()` → increment frame counter

**Responsibilities**:
- Blink LED at 2Hz (visual indicator)
- Increment frame counter (atomic, for verification)
- Process incoming messages from queue
- Send responses to outgoing queue

**Message Handling**:
- Parse `M!{...}\n` format
- Handle `get_frame_count` command
- Handle `echo` command
- Send responses to outgoing queue

### 4. I/O Task

**Responsibilities**:
- Drain outgoing queue and send via serial
- Read from serial and push to incoming queue (filter `M!` prefix)
- Handle serial state (Ready/Disconnected/Error)
- Retry serial initialization if disconnected

**State Management**:
- `Uninitialized` → Try init → `Ready` or `Error`
- `Ready` → Read/write error → `Disconnected`
- `Disconnected` → Periodic retry → `Ready` or `Error`

### 5. Host-Side Test Automation (fw-tests)

**Test Scenarios**:
1. **Start without serial**: Flash → Wait → Connect → Query frame count → Verify increase
2. **Start with serial**: Flash → Connect immediately → Query → Disconnect → Reconnect → Verify
3. **Echo test**: Connect → Echo → Disconnect → Reconnect → Echo → Verify

**Tools**:
- `serialport` crate for serial communication
- `cargo-espflash` for flashing/resetting
- Message parser for `M!{...}\n` format
- Tokio test framework

## Design Decisions

1. **Use embassy-sync channels**: Already in dependency tree, no_std, async-compatible, MPMC
2. **Frame counter over blink count**: Better verification of continuous operation
3. **Message prefix `M!`**: Filters out debug prints, reusable for main system
4. **External discriminators**: Efficient with serde-json-core, cleaner API
5. **Poll-based serial detection**: Simpler, more reliable than interrupts
6. **Replace test_usb**: Current implementation is broken, start fresh

## Future Extensions

- **Multi-transport**: Can extend MessageRouter to handle multiple transports
- **Main system integration**: MessageRouter pattern can be used in production code
- **SerialTransport update**: Add `M!` prefix support to filter debug output
