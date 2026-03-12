# Design: LP-CLI Serial Firmware Connection

## Scope of Work

Connect `lp-cli` to the real ESP32 firmware (`fw-esp32`) via serial communication. This involves:

1. **Serial Port Detection & Selection**
   - Auto-detect serial ports (or allow manual specification)
   - Support `lp-cli --push serial` (auto-detect) or `lp-cli --push serial:/dev/cu...` (manual)
   - Filter to only `/dev/cu.*` devices (ignore `/dev/tty.*`)
   - If multiple unique ports found, use `dialoguer` for interactive selection
   - Parse baud rate from query string: `serial:/dev/cu.X?baud=115200`

2. **Serial Transport Implementation**
   - Implement `HostSpecifier::Serial` in `client_connect()` (currently returns "not yet implemented")
   - Create async serial transport using `tokio-serial`
   - Use the existing `AsyncSerialClientTransport` pattern (similar to emulator transport)
   - Handle message framing (M! prefix) and newline-delimited JSON
   - Log non-M! lines to stderr with prefix (e.g., `[serial]` or `[fw-debug]`)

3. **Project Management Commands**
   - Add `StopAllProjects` server command
   - Before pushing a project, stop all currently loaded projects on the server
   - Load the newly pushed project

## File Structure

```
lp-core/lp-model/src/
├── message.rs                          # UPDATE: Add StopAllProjects to ClientRequest
└── server/
    └── api.rs                          # UPDATE: Add StopAllProjects to ServerMessagePayload

lp-core/lp-server/src/
├── handlers.rs                         # UPDATE: Add handle_stop_all_projects()
└── project_manager.rs                  # UPDATE: Add unload_all_projects() method

lp-core/lp-client/src/
├── specifier.rs                        # UPDATE: Add baud_rate field to Serial variant, parse query string
├── transport_serial/
│   ├── mod.rs                          # UPDATE: Export create_hardware_serial_transport_pair
│   └── hardware.rs                     # NEW: Hardware serial transport factory
└── client.rs                           # UPDATE: Add stop_all_projects() method

lp-cli/src/
├── client/
│   ├── client_connect.rs               # UPDATE: Implement Serial transport creation
│   └── serial_port.rs                  # NEW: Serial port detection and selection
└── commands/
    └── dev/
        └── handler.rs                  # UPDATE: Call stop_all_projects() before push

lp-cli/Cargo.toml                       # UPDATE: Add tokio-serial, dialoguer, serialport dependencies
```

## Conceptual Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    lp-cli dev --push serial                 │
└────────────────────┬────────────────────────────────────────┘
                     │
                     ▼
         ┌───────────────────────┐
         │  Serial Port Detection │
         │  - List cu.* ports     │
         │  - Filter/select       │
         │  - Parse baud rate     │
         └───────────┬────────────┘
                     │
                     ▼
         ┌───────────────────────┐
         │   client_connect()     │
         │   HostSpecifier::Serial│
         └───────────┬────────────┘
                     │
                     ▼
    ┌────────────────────────────────────┐
    │  create_hardware_serial_transport() │
    │  - Opens tokio-serial port         │
    │  - Creates backend thread          │
    │  - Returns AsyncSerialClientTransport│
    └────────────┬───────────────────────┘
                 │
                 ▼
    ┌────────────────────────────────────┐
    │   Backend Thread Loop               │
    │   - Reads lines from serial         │
    │   - Filters M! prefix              │
    │   - Logs non-M! lines with prefix  │
    │   - Parses JSON → ServerMessage     │
    │   - Sends via channel               │
    │   - Writes ClientMessage with M!   │
    └────────────┬───────────────────────┘
                 │
                 ▼
    ┌────────────────────────────────────┐
    │   AsyncSerialClientTransport       │
    │   (channels to backend thread)     │
    └────────────┬───────────────────────┘
                 │
                 ▼
    ┌────────────────────────────────────┐
    │   LpClient                          │
    │   - stop_all_projects()             │
    │   - push_project_async()            │
    │   - project_load()                  │
    └─────────────────────────────────────┘
```

## Main Components and Interactions

### 1. Serial Port Detection (`lp-cli/src/client/serial_port.rs`)

- Lists `/dev/cu.*` ports using `serialport::available_ports()`
- Filters to only `cu.*` devices (ignores `tty.*`)
- Uses `dialoguer` for interactive selection if multiple found
- Parses baud rate from query string (defaults to 115200)
- Returns port name and baud rate

### 2. HostSpecifier Updates

- Update `Serial` variant: `Serial { port: Option<String>, baud_rate: Option<u32> }`
- Parse query string: `serial:/dev/cu.X?baud=115200`
- Default `baud_rate` to 115200 if not specified
- Update `parse()` method to handle query strings

### 3. Hardware Serial Transport (`transport_serial/hardware.rs`)

- Factory function: `create_hardware_serial_transport_pair(port: &str, baud_rate: u32)`
- Backend thread runs blocking serial I/O loop
- Uses `tokio-serial` for async serial operations
- Reads lines, filters for `M!` prefix
- Logs non-M! lines to stderr with `[serial]` prefix
- Parses JSON messages and sends via channel
- Writes `ClientMessage` with `M!` prefix and newline

### 4. StopAllProjects Command

- New `ClientRequest::StopAllProjects` variant
- New `ServerMessagePayload::StopAllProjects` variant
- Handler calls `project_manager.unload_all_projects()`
- Add `unload_all_projects()` method to `ProjectManager`
- Client method: `client.stop_all_projects()`

### 5. Dev Command Handler

- Before `push_project_async()`, call `client.stop_all_projects()`
- Then proceed with normal push and load flow
- Handle errors appropriately
