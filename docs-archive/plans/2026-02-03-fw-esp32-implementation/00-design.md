# fw-esp32 Implementation - Design

## Scope of Work

Implement a working ESP32 firmware (`fw-esp32`) that:
1. Runs on real ESP32-C6 hardware
2. Implements the RMT driver for WS2811/WS2812 LEDs (based on reference from lpmini2024)
3. Implements a complete OutputProvider using the RMT driver
4. Implements USB serial I/O for communication
5. Implements the server loop (similar to fw-emu but adapted for ESP32 async runtime)
6. Implements time provider for ESP32
7. Can be built and flashed to hardware
8. Includes test features (e.g., `test_rmt`) for manual verification

**Note**: `fw-esp32` is for real hardware only. `fw-emu` is what runs in the emulator for testing.

## File Structure

```
lp-fw/fw-esp32/
├── Cargo.toml                    # UPDATE: Add test_rmt feature, dependencies
├── src/
│   ├── main.rs                   # UPDATE: Initialize all components, start server loop
│   ├── board/
│   │   ├── mod.rs                # (existing)
│   │   └── esp32c6.rs           # (existing)
│   ├── output/
│   │   ├── mod.rs                # NEW: OutputProvider implementation
│   │   ├── rmt_driver.rs         # NEW: RMT driver for WS2811/WS2812
│   │   └── provider.rs           # NEW: Esp32OutputProvider
│   ├── serial/
│   │   ├── mod.rs                # UPDATE: Re-export Esp32UsbSerialIo
│   │   └── usb_serial.rs         # UPDATE: Implement async USB serial SerialIo
│   ├── server_loop.rs            # NEW: Async server loop implementation
│   └── time.rs                   # NEW: ESP32 TimeProvider implementation
└── tests/                        # NEW: Test features
    └── test_rmt.rs               # NEW: RMT test mode (rainbow pattern)
```

## Conceptual Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                     Embassy Runtime                          │
│                                                              │
│  ┌──────────────────────────────────────────────────────┐   │
│  │  async fn main()                                     │   │
│  │    ├─ Initialize board (clock, heap, runtime)       │   │
│  │    ├─ Initialize logger                              │   │
│  │    ├─ Initialize USB serial                          │   │
│  │    ├─ Initialize OutputProvider (RMT driver)         │   │
│  │    ├─ Initialize TimeProvider                        │   │
│  │    ├─ Create LpServer                                │   │
│  │    └─ Run server loop (async)                        │   │
│  └──────────────────────────────────────────────────────┘   │
│                                                              │
│  ┌──────────────────────────────────────────────────────┐   │
│  │  async fn run_server_loop()                          │   │
│  │    Loop:                                             │   │
│  │      ├─ Read serial messages (async, non-blocking) │   │
│  │      ├─ Call server.tick() (sync)                   │   │
│  │      ├─ Send responses (async)                       │   │
│  │      └─ Yield to Embassy                            │   │
│  └──────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                    Hardware Layer                           │
│                                                              │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐    │
│  │ USB Serial   │  │ RMT Driver   │  │ Timer        │    │
│  │ (Async)      │  │ (Interrupt)  │  │ (embassy-time)│    │
│  └──────────────┘  └──────────────┘  └──────────────┘    │
│       │                  │                  │              │
│       └──────────────────┴──────────────────┘              │
│                          │                                 │
│                          ▼                                 │
│              ┌────────────────────┐                        │
│              │   ESP32 Hardware   │                        │
│              │  (GPIO, RMT, USB)  │                        │
│              └────────────────────┘                        │
└─────────────────────────────────────────────────────────────┘
```

## Main Components and Interactions

### 1. Main Entry (`main.rs`)

- **Purpose**: Initialize all components and start the server loop
- **Key responsibilities**:
  - Initialize board (clock, heap, Embassy runtime)
  - Initialize logger (esp_println)
  - Initialize USB serial
  - Initialize OutputProvider (with RMT driver)
  - Initialize TimeProvider
  - Create LpServer with OutputProvider and filesystem
  - Create SerialTransport with USB serial
  - Start async server loop

### 2. Server Loop (`server_loop.rs`)

- **Purpose**: Main async loop that processes messages and calls server.tick()
- **Key responsibilities**:
  - Loop forever:
    - Read available serial messages (async, non-blocking)
    - Collect messages into a vector
    - Calculate delta time since last tick
    - Call `server.tick(delta_ms, messages)` (synchronous)
    - Send responses via serial (async)
    - Yield to Embassy runtime (allows other tasks)
  - Similar structure to fw-emu, but async

### 3. Output Provider (`output/provider.rs`)

- **Purpose**: Implement OutputProvider trait using RMT driver
- **Key responsibilities**:
  - Track opened channels (pin -> handle mapping)
  - On `open()`: Initialize RMT driver for the given pin
  - On `write()`: Write LED data to RMT driver
  - On `close()`: Clean up RMT channel
  - Store RMT transaction handles (must be kept alive)

### 4. RMT Driver (`output/rmt_driver.rs`)

- **Purpose**: Low-level WS2811/WS2812 LED driver using ESP32 RMT peripheral
- **Key responsibilities**:
  - Initialize RMT channel for a specific GPIO pin
  - Convert RGB data to RMT pulse codes
  - Use interrupt-driven double buffering
  - Handle transmission start/stop
  - Wait for frame completion
- **Based on**: Reference implementation from lpmini2024
- **Adaptations**: 
  - Integrate with OutputProvider API
  - Support dynamic pin configuration
  - Store transaction handle to keep it alive

### 5. USB Serial (`serial/usb_serial.rs`)

- **Purpose**: Implement SerialIo trait using ESP32 USB-serial (Async mode)
- **Key responsibilities**:
  - Split USB serial into rx/tx halves
  - Implement `write()`: Blocking write using async USB serial (use `block_on` or similar)
  - Implement `read_available()`: Non-blocking read (check available, read what's there)
  - Implement `has_data()`: Check if data is available
- **Challenge**: Bridge async USB serial to synchronous SerialIo trait

### 6. Time Provider (`time.rs`)

- **Purpose**: Implement TimeProvider trait using ESP32 timers
- **Key responsibilities**:
  - Use `embassy_time::Instant` for time tracking
  - Implement `now_ms()`: Get current time in milliseconds
  - Implement `elapsed_ms()`: Calculate elapsed time
- **Implementation**: Use embassy-time APIs

### 7. Test Features (`tests/test_rmt.rs`)

- **Purpose**: Test RMT driver independently of LightPlayer engine
- **Key responsibilities**:
  - When `test_rmt` feature is enabled, bypass normal server loop
  - Initialize RMT driver
  - Run simple test patterns (rainbow, chase, solid color)
  - Allow visual verification of LED output
- **Usage**: `cargo run --features test_rmt`

## Component Interactions

1. **main.rs → server_loop.rs**: Creates server, transport, time provider, calls `run_server_loop()`
2. **server_loop.rs → SerialTransport**: Reads/writes messages via SerialIo
3. **server_loop.rs → LpServer**: Calls `tick()` with messages and delta time
4. **LpServer → OutputProvider**: Opens channels, writes LED data
5. **OutputProvider → RMT Driver**: Initializes channels, writes data
6. **RMT Driver → ESP32 Hardware**: Configures RMT peripheral, sends pulses to GPIO
7. **SerialTransport → USB Serial**: Reads/writes bytes via SerialIo
8. **USB Serial → ESP32 Hardware**: Uses USB-serial peripheral

## Key Design Decisions

1. **Async Runtime**: Use async main loop that yields to Embassy between iterations
2. **Serial I/O**: Bridge async USB serial to synchronous SerialIo trait using blocking wrappers
3. **RMT Driver**: Adapt reference implementation to work with OutputProvider API
4. **Time Provider**: Use embassy-time for millisecond-precision timing
5. **Testing**: Add test features for manual verification (human-in-the-loop)
6. **Pin Configuration**: Pass pin number from OutputProvider::open() to RMT driver

## Dependencies

- `esp-hal`: ESP32 hardware abstraction
- `embassy-executor`: Async runtime
- `embassy-time`: Time provider
- `fw-core`: Serial I/O and transport abstractions
- `lp-server`: LightPlayer server implementation
- `lp-shared`: OutputProvider trait, OutputFormat enum
- `smart_leds`: RGB8 type for LED data

## Notes

- RMT driver uses unsafe code for direct hardware register access
- RMT transaction handle must be kept alive (stored in OutputProvider)
- USB serial uses Async driver mode, needs bridging to sync SerialIo trait
- Test features allow manual verification without full server stack
- Similar structure to fw-emu, but adapted for async runtime and real hardware
