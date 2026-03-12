# Notes: LP-CLI Serial Firmware Connection

## Scope of Work

Connect `lp-cli` to the real ESP32 firmware (`fw-esp32`) via serial communication. This involves:

1. **Serial Port Detection & Selection**
   - Auto-detect serial ports (or allow manual specification)
   - Support `lp-cli --push serial` (auto-detect) or `lp-cli --push serial:/dev/cu...` (manual)
   - Handle multiple ports intelligently: `/dev/cu.X` and `/dev/tty.X` are the same device, prefer `cu.X`
   - If multiple unique ports found, prompt user to select

2. **Serial Transport Implementation**
   - Implement `HostSpecifier::Serial` in `client_connect()` (currently returns "not yet implemented")
   - Create async serial transport using `tokio-serial` or similar
   - Use the existing `AsyncSerialClientTransport` pattern (similar to emulator transport)
   - Handle message framing (M! prefix) and newline-delimited JSON

3. **Project Management Commands**
   - Before pushing a project, stop all currently loaded projects on the server
   - Load the newly pushed project
   - May need new server command: `StopAllProjects` or similar
   - Or use existing `ListLoadedProjects` + `UnloadProject` for each

## Current State of the Codebase

### Serial Transport Infrastructure

- **`HostSpecifier::Serial`** exists in `lp-core/lp-client/src/specifier.rs`:
  - Supports `serial:auto` (None port) and `serial:/dev/...` (Some port)
  - Parsing is implemented

- **`AsyncSerialClientTransport`** exists in `lp-core/lp-client/src/transport_serial/client.rs`:
  - Generic async transport using channels
  - Works with backend thread (similar to emulator)
  - Factory pattern for creating transports

- **Emulator transport** (`lp-core/lp-client/src/transport_serial/emulator.rs`):
  - Shows pattern for creating serial transport pairs
  - Uses `create_emulator_serial_transport_pair()` factory function

- **`client_connect()`** in `lp-cli/src/client/client_connect.rs`:
  - Currently returns `bail!("Serial transport not yet implemented")` for `HostSpecifier::Serial`
  - Has working implementations for `Local`, `WebSocket`, and `Emulator`

### Serial Port Detection

- **`serialport` crate** is used in `lp-fw/fw-tests/src/test_usb_helpers.rs`:
  - `serialport::available_ports()` for listing ports
  - Filters for `/dev/cu.usbmodem*` on macOS
  - Uses `serialport::new()` to open ports

- **`lp-cli` dependencies** (`lp-cli/Cargo.toml`):
  - Does NOT currently have `serialport` or `tokio-serial` dependencies
  - Has `serial` feature flag (enabled by default)
  - Uses `lp-client` with `serial` feature

### Project Management

- **Server commands** (`lp-model/src/message.rs`):
  - `LoadProject { path }` - loads a project
  - `UnloadProject { handle }` - unloads a project by handle
  - `ListLoadedProjects` - lists all loaded projects with handles
  - `ListAvailableProjects` - lists available projects on filesystem
  - **No `StopAllProjects` command exists**

- **Project Manager** (`lp-core/lp-server/src/project_manager.rs`):
  - `load_project()` - loads a project, returns handle
  - `unload_project(handle)` - unloads by handle
  - `list_loaded_projects()` - returns `Vec<(ProjectHandle, String)>` (handle + name)
  - Projects are tracked by handle (u32) and name

- **Client API** (`lp-core/lp-client/src/client.rs`):
  - `project_load(path)` - loads a project
  - `project_unload(handle)` - unloads a project
  - `list_loaded_projects()` - lists loaded projects
  - `list_available_projects()` - lists available projects

### Firmware Serial Protocol

- **Message format** (`lp-fw/fw-esp32/src/serial/io_task.rs`):
  - Messages prefixed with `M!` followed by JSON
  - Newline-delimited (`\n` after each message)
  - Non-M! lines are ignored (debug output)

- **Baud rate**: Likely 115200 (standard for ESP32), but need to verify

### Current Dev Command Flow

- **`lp-cli dev --push <host>`** (`lp-cli/src/commands/dev/handler.rs`):
  1. Validates local project (reads `project.json`)
  2. Parses host specifier
  3. Connects via `client_connect()`
  4. Pushes project files to server
  5. Loads project via `client.project_load()`
  6. Starts file watching loop
  7. Runs UI or waits for Ctrl+C

## Questions That Need to be Answered

### Q1: Serial Port Detection Strategy ✅ ANSWERED

**Question**: How should we handle serial port detection and selection?

**Context**: 
- macOS has both `/dev/cu.*` (callout) and `/dev/tty.*` (terminal) devices for the same physical port
- `cu.*` is preferred for non-interactive use (doesn't wait for carrier signal)
- Multiple ESP32 devices might be connected
- User might want to specify exact port

**Answer**:
- Use `serialport::available_ports()` to list all ports
- **Filter to only `/dev/cu.*` devices** (ignore `/dev/tty.*` completely - they're the same device, and `cu.*` is preferred)
- If multiple unique `cu.*` devices found, use `dialoguer` for interactive selection
- If single device found, use it automatically
- If no devices found, error with helpful message
- Allow manual override via `serial:/dev/cu...` syntax
- Use `dialoguer` crate for interactive port selection (user confirmed)

### Q2: Serial Transport Implementation Details ✅ ANSWERED

**Question**: What library should we use for async serial I/O, and how should we structure the backend thread?

**Context**:
- Need async serial I/O (non-blocking reads/writes)
- `serialport` crate is synchronous
- `tokio-serial` provides async wrapper
- Backend thread pattern already exists for emulator

**Answer**:
- Use `tokio-serial` for async serial I/O (user confirmed)
- Create `create_hardware_serial_transport_pair()` factory function (similar to emulator)
- Use `AsyncSerialClientTransport` pattern (backend thread with channels)
- Backend thread can use tokio-serial's async interface within a tokio runtime, or use blocking interface
- Follow the emulator transport pattern for consistency

### Q3: Message Framing and Protocol ✅ ANSWERED

**Question**: How should we handle the M! prefix and newline framing in the serial transport?

**Context**:
- Firmware expects `M!<json>\n` format
- Non-M! lines are debug output (should be filtered or logged?)
- Need to handle partial reads, buffering

**Answer**:
- Backend thread reads lines, filters for `M!` prefix
- Strips `M!` prefix before parsing JSON
- Sends parsed `ServerMessage` via channel
- Client side sends `ClientMessage`, adds `M!` prefix and `\n` before writing
- Handle buffering for partial reads
- **Log non-M! lines to stderr with a prefix** (e.g., `[serial]` or `[fw-debug]`) for debugging (user confirmed)

### Q4: Stop All Projects Before Push ✅ ANSWERED

**Question**: How should we stop all currently loaded projects before pushing a new one?

**Context**:
- No `StopAllProjects` command exists
- Can use `ListLoadedProjects` + `UnloadProject` for each
- Should this be automatic, or optional flag?

**Answer**:
- **Create a new `StopAllProjects` server command** (user confirmed)
- Add `StopAllProjects` variant to `ClientRequest` enum
- Add `StopAllProjects` variant to `ServerMessagePayload` enum
- Implement handler in `lp-server/src/handlers.rs` that calls `project_manager.unload_all()` or similar
- Add `project_unload_all()` method to `ProjectManager` if needed
- Add `client.stop_all_projects()` method to `LpClient`
- Call `stop_all_projects()` automatically before `push_project_async()` in dev command handler

### Q5: Baud Rate and Serial Settings ✅ ANSWERED

**Question**: What baud rate and serial settings should we use?

**Context**:
- ESP32 typically uses 115200 baud
- Need to match firmware settings
- Other settings: data bits, stop bits, parity, flow control

**Answer**:
- **Use query string in URI** (user confirmed): `serial:/dev/cu.usbmodem2101?baud=115200` or `serial:auto?baud=115200`
- Default to 115200 baud if not specified
- Update `HostSpecifier::Serial` to include optional `baud_rate: Option<u32>` field
- Parse query string in `HostSpecifier::parse()` for serial specifiers
- 8 data bits, 1 stop bit, no parity (8N1) - standard settings
- No flow control
- Query string approach is extensible for future options (e.g., `?baud=115200&timeout=1000`)

### Q6: Error Handling and Reconnection ✅ ANSWERED

**Question**: How should we handle serial connection errors and disconnections?

**Context**:
- Serial port might disconnect during use
- USB serial can be unplugged
- Should we attempt reconnection, or fail fast?

**Answer**:
- **Fail immediately** if port unavailable (no retry logic for initial connection)
- **Disconnect immediately** on errors during use (no auto-reconnect)
- Log clear error messages
- Transport should close cleanly on error
- Can add auto-reconnect option later if needed (user confirmed)
