# Firmware Architecture Design

## Scope of Work

Implement the firmware architecture separation to enable testing firmware code without hardware. This involves:

1. **Create `fw-core` crate** (`lp-app/crates/fw-core`)
   - Generic `no_std` server loop utilities
   - Serial transport implementation using `SerialIo` trait
   - Shared utilities for firmware

2. **Create `fw-esp32` app** (`lp-app/apps/fw-esp32`)
   - ESP32-specific hardware initialization
   - ESP32 USB-serial implementation of `SerialIo`
   - ESP32 output provider implementation
   - Main entry point using Embassy async runtime

3. **Create `fw-emu` app** (`lp-app/apps/fw-emu`)
   - RISC-V32 emulator integration
   - Syscall-based `SerialIo`, `TimeProvider`, and `OutputProvider` implementations
   - Main entry point for emulator execution

4. **Extend `lp-shared`**
   - Add `TimeProvider` trait

## File Structure

```
lp-app/
├── crates/
│   ├── fw-core/                          # NEW: Firmware core library
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── serial/                   # NEW: Serial I/O abstractions
│   │       │   ├── mod.rs
│   │       │   └── io.rs                 # NEW: SerialIo trait
│   │       ├── transport/                # NEW: Serial transport implementation
│   │       │   ├── mod.rs
│   │       │   └── serial.rs             # NEW: SerialTransport using SerialIo
│   │       └── util.rs                   # NEW: Helper utilities for server loop
│   │
│   └── lp-shared/
│       └── src/
│           └── time/                     # NEW: Time provider abstraction
│               ├── mod.rs
│               └── provider.rs          # NEW: TimeProvider trait
│
└── apps/
    ├── fw-esp32/                         # NEW: ESP32 firmware app
    │   ├── Cargo.toml
    │   └── src/
    │       ├── main.rs                   # NEW: Embassy async main entry point
    │       ├── board/                    # NEW: Board-specific code
    │       │   ├── mod.rs
    │       │   └── esp32c6.rs            # NEW: ESP32-C6 specific initialization
    │       ├── serial/                   # NEW: ESP32 USB-serial implementation
    │       │   ├── mod.rs
    │       │   └── usb_serial.rs         # NEW: USB-serial SerialIo implementation
    │       ├── output/                   # NEW: ESP32 output provider
    │       │   ├── mod.rs
    │       │   └── provider.rs           # NEW: OutputProvider implementation
    │       └── server_loop.rs            # NEW: Main server loop for ESP32
    │
    └── fw-emu/                           # NEW: Emulator firmware app
        ├── Cargo.toml
        └── src/
            ├── main.rs                   # NEW: Emulator entry point
            ├── serial/                   # NEW: Serial I/O via syscalls
            │   ├── mod.rs
            │   └── syscall.rs            # NEW: SerialIo using syscalls (read/write)
            ├── time/                     # NEW: Time provider via syscalls
            │   ├── mod.rs
            │   └── syscall.rs            # NEW: TimeProvider using syscalls
            ├── output/                   # NEW: Output provider via syscalls
            │   ├── mod.rs
            │   └── syscall.rs            # NEW: OutputProvider using syscalls
            └── server_loop.rs            # NEW: Main server loop for emulator
```

## Conceptual Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                      Firmware Apps                          │
├──────────────────────────┬──────────────────────────────────┤
│      fw-esp32           │          fw-emu                  │
│  (ESP32 Hardware)       │    (RISC-V Emulator)            │
├──────────────────────────┼──────────────────────────────────┤
│ • Embassy async main     │ • Emulator main                  │
│ • USB-serial SerialIo    │ • SerialIo via syscalls          │
│ • ESP32 output provider  │ • OutputProvider via syscalls    │
│ • Board-specific init    │ • TimeProvider via syscalls      │
│ • Server loop            │ • Server loop                    │
└──────────┬───────────────┴──────────────┬──────────────────┘
           │                              │
           │                              │ (syscalls)
           │                              ▼
           │                    ┌──────────────────┐
           │                    │   Host Process    │
           │                    │  (Emulator Host)  │
           │                    │ • Handle syscalls │
           │                    │ • Provide time   │
           │                    │ • Display LEDs   │
           │                    │ • Serial I/O      │
           │                    └──────────────────┘
           │
           └──────────┬────────────────────┘
                      │
        ┌─────────────▼──────────────┐
        │      fw-core               │
        │  (Shared Firmware Code)   │
        ├────────────────────────────┤
        │ • SerialIo trait           │
        │ • SerialTransport         │
        │ • Helper utilities         │
        └─────────────┬──────────────┘
                      │
        ┌─────────────▼──────────────┐
        │    lp-shared               │
        │  (Shared Abstractions)     │
        ├────────────────────────────┤
        │ • TimeProvider trait       │
        │ • LpFs trait               │
        │ • OutputProvider trait     │
        │ • ServerTransport trait    │
        └─────────────┬──────────────┘
                      │
        ┌─────────────▼──────────────┐
        │      lp-server             │
        │  (Server Logic)            │
        ├────────────────────────────┤
        │ • LpServer::tick()         │
        │ • Project management      │
        │ • Message handling        │
        └────────────────────────────┘
```

## Main Components

### fw-core (`lp-app/crates/fw-core`)

**Purpose**: Generic `no_std` code shared between firmware implementations.

**Components**:
1. **SerialIo Trait** (`serial/io.rs`)
   - `write(data: &[u8]) -> Result<(), SerialError>` - Blocking write
   - `read_available(buf: &mut [u8]) -> Result<usize, SerialError>` - Non-blocking read
   - `has_data() -> bool` - Optional optimization hint

2. **SerialTransport** (`transport/serial.rs`)
   - Implements `ServerTransport` trait
   - Uses `SerialIo` for raw byte I/O
   - Handles message framing (JSON + `\n` termination)
   - Buffers partial reads until complete message
   - Parses JSON and handles errors

3. **Helper Utilities** (`util.rs`)
   - Frame timing utilities
   - Message processing helpers
   - (Not the main loop - that stays in firmware apps)

### lp-shared (`lp-app/crates/lp-shared`)

**New Component**:
- **TimeProvider Trait** (`time/provider.rs`)
  - `now_ms() -> u64` - Get current time in milliseconds since boot
  - `elapsed_ms(start: u64) -> u64` - Calculate elapsed time

### fw-esp32 (`lp-app/apps/fw-esp32`)

**Purpose**: ESP32-specific firmware application.

**Components**:
1. **Board-Specific Code** (`board/esp32c6.rs`)
   - ESP32-C6 specific initialization
   - Feature-gated (can add more boards later)
   - Hardware configuration

2. **USB-Serial SerialIo** (`serial/usb_serial.rs`)
   - Implements `SerialIo` trait
   - Uses ESP32 USB-serial (not hardware UART)
   - Blocking writes, non-blocking reads

3. **ESP32 Output Provider** (`output/provider.rs`)
   - Implements `OutputProvider` trait
   - GPIO/LED driver code
   - Hardware-specific output handling

4. **Main Entry Point** (`main.rs`)
   - Embassy async runtime setup
   - Hardware initialization
   - Creates `LpServer` with `LpFsMemory` and output provider
   - Creates `SerialTransport` with USB-serial `SerialIo`
   - Runs server loop

5. **Server Loop** (`server_loop.rs`)
   - Main loop that handles hardware I/O
   - Calls `server.tick()` with incoming messages
   - Handles frame timing
   - Sends responses via transport

### fw-emu (`lp-app/apps/fw-emu`)

**Purpose**: Firmware application that runs in RISC-V32 emulator for testing.

**Components**:
1. **Syscall-Based SerialIo** (`serial/syscall.rs`)
   - Implements `SerialIo` trait
   - Uses syscalls for read/write
   - Initially `todo!()`, implement after basic structure

2. **Syscall-Based TimeProvider** (`time/syscall.rs`)
   - Implements `TimeProvider` trait
   - Uses syscalls to get time from host
   - Initially `todo!()`, implement after basic structure

3. **Syscall-Based OutputProvider** (`output/syscall.rs`)
   - Implements `OutputProvider` trait
   - Uses syscalls to send LED data to host
   - Initially `todo!()`, implement after basic structure

4. **Main Entry Point** (`main.rs`)
   - Emulator integration
   - Creates `LpServer` with `LpFsMemory` and syscall output provider
   - Creates `SerialTransport` with syscall `SerialIo`
   - Runs server loop in emulator

5. **Server Loop** (`server_loop.rs`)
   - Main loop for emulator execution
   - Calls `server.tick()` with incoming messages
   - Handles frame timing
   - Sends responses via transport

## Message Flow

```
Host → USB-serial → SerialIo → SerialTransport → LpServer → OutputProvider
```

For `fw-emu`:
```
Host → Syscalls → SerialIo → SerialTransport → LpServer → OutputProvider → Syscalls → Host
```

## Key Design Decisions

1. **SerialIo is synchronous**: Simple interface that works with both blocking and async implementations
2. **Transport handles complexity**: Message framing, buffering, JSON parsing in `SerialTransport`
3. **Main loops in firmware apps**: Each app handles its own hardware I/O and runtime
4. **Syscalls for fw-emu**: Uses emulator syscalls for realistic host interaction
5. **Board-specific code isolated**: ESP32-C6 code in separate file for easy copy-paste to new boards
6. **Feature flags for variants**: Higher-level gating (function level) where possible

## Implementation Notes

- Start with `LpFsMemory` for both firmware apps (add real filesystem later)
- Syscall implementations in `fw-emu` can be `todo!()` initially
- Focus on getting basic structure working first
- Extend emulator syscall support as needed in later phases
