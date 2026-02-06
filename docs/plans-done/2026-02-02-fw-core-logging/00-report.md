# fw-core Logging System Study

## Problem Statement

`fw-core` needs a logging system that works across multiple environments:

- **no_std** and **std** environments
- **Emulator** (syscall-based prints routed to host)
- **ESP32 device** (hardware-specific serial output)
- **Host** (standard stdout/stderr)
- Support for **debug, info, warn, error** levels
- **Scoped debugging** similar to TypeScript's `DEBUG=serial` pattern

## Current State Analysis

### Existing Logging Infrastructure

1. **lp-shared/src/log/mod.rs**
   - Uses standard `log` crate (version 0.4, `default-features = false`)
   - Only provides test logger implementation
   - No production logger implementation
   - Comment notes: "Firmware (fw-host, device) is responsible for initializing the logger"

2. **fw-core Current State**
   - No logging infrastructure currently
   - Comments in `transport/serial.rs` note: "In no_std, we can't easily log, so just return None"
   - Uses `#![no_std]` with `alloc` feature

3. **ESP32 (fw-esp32)**
   - Uses `esp_println::println!` directly
   - Initializes with `esp_println::logger::init_logger_from_env()`
   - No structured logging levels or scoping

4. **Emulator (fw-emu)**
   - Uses `host_debug!` macro from `lp-glsl-builtins`
   - Syscalls available:
     - `SYSCALL_WRITE` (always prints)
     - `SYSCALL_DEBUG` (currently routes to host's `debug!` macro)
   - Emulator routes `SYSCALL_DEBUG` to host's `debug!` macro which checks `DEBUG=1` env var

5. **Ad-hoc Debug Macros**
   - `lp-glsl-compiler/src/debug.rs`: Checks `DEBUG=1` env var, only works with `std`
   - `lp-riscv-inst/src/debug.rs`: Similar pattern
   - Various other debug macros scattered throughout codebase

### Current Limitations

1. **No unified logging API** - Each environment uses different approaches
2. **No scoping support** - Can't filter by module/component (e.g., `DEBUG=serial`)
3. **No level support** - Only binary debug on/off
4. **Inconsistent** - Different macros and patterns across codebase
5. **fw-core can't log** - Explicitly noted as limitation in code comments

## Requirements Analysis

### Core Requirements

1. **Multi-environment support**
   - `no_std` with `alloc` (fw-core default)
   - `std` (host environments)
   - Emulator (syscall-based)
   - ESP32 (hardware serial)

2. **Log Levels**
   - `debug` - Detailed debugging information
   - `info` - General informational messages
   - `warn` - Warning messages
   - `error` - Error conditions

3. **Scoped Debugging**
   - Support `DEBUG=serial` style filtering
   - Multiple scopes: `DEBUG=serial,transport`
   - Wildcard support: `DEBUG=*` (all scopes)
   - Case-insensitive matching

4. **Performance**
   - Zero-cost when disabled (compile-time elimination)
   - Minimal runtime overhead when enabled
   - No allocation in hot paths (when possible)

5. **API Design**
   - Simple macro-based API: `lp_debug!("scope", "message", args)`
   - Consistent across all environments
   - Format string support (like `println!`)

## Standard Rust Logging Libraries Comparison

### 1. `log` Crate (Standard Library)

**Pros:**

- Standard Rust logging facade
- `no_std` compatible (with `default-features = false`)
- Widely used and well-maintained
- Provides `debug!`, `info!`, `warn!`, `error!` macros
- Zero-cost when disabled (compile-time elimination via `max_level_*` features)

**Cons:**

- Requires separate logger implementation per environment
- No built-in scoping support (would need custom implementation)
- Logger must be set at runtime (not link-time)
- Metadata filtering is limited (module path, level only)

**Verdict:** Good foundation, but needs custom scoping layer

### 2. `tracing` Crate

**Pros:**

- Structured logging with spans and events
- More powerful than `log`
- Supports scoping through spans
- Can be used with `log` as a bridge

**Cons:**

- `tracing-subscriber` requires `std` (not `no_std` compatible)
- More complex API
- Heavier weight than needed for this use case
- Would require custom `no_std` subscriber implementation

**Verdict:** Overkill for this use case, and `no_std` support is limited

### 3. `defmt` Crate

**Pros:**

- Specifically designed for embedded systems
- Highly efficient (deferred formatting)
- Works with probe-rs/espflash

**Cons:**

- Requires specific tooling (probe-rs, defmt-print)
- Not suitable for host/emulator environments
- Doesn't match the multi-environment requirement
- No built-in scoping support

**Verdict:** Not suitable for multi-environment use case

### 4. Custom Solution

**Pros:**

- Full control over API and behavior
- Can optimize for specific use cases
- Can support all environments uniformly
- Can implement scoping exactly as needed

**Cons:**

- More code to maintain
- Need to implement all features ourselves
- Less standardized

**Verdict:** Most flexible, but more work

## Design Considerations

### Architecture Options

#### Option 1: `log` Crate + Custom Scoping Layer

**Structure:**

```
lp-log/
├── src/
│   ├── lib.rs              # Public API macros
│   ├── scoped.rs           # Scoping logic
│   └── backend/
│       ├── mod.rs
│       ├── std.rs          # std implementation
│       ├── emu.rs          # emulator syscall implementation
│       └── esp32.rs        # ESP32 implementation (optional)
```

**Pros:**

- Leverages standard `log` crate
- Familiar API for Rust developers
- Can use existing `log`-compatible libraries
- Zero-cost when disabled via `max_level_*` features

**Cons:**

- Need custom scoping implementation
- Logger initialization required at runtime
- Scoping might not integrate cleanly with `log`'s metadata system

#### Option 2: Fully Custom Solution

**Structure:**

```
lp-log/
├── src/
│   ├── lib.rs              # Public API macros
│   ├── level.rs            # Log levels
│   ├── scope.rs            # Scoping logic
│   ├── backend.rs          # Backend trait
│   └── backends/
│       ├── mod.rs
│       ├── std.rs
│       ├── emu.rs
│       └── esp32.rs
```

**Pros:**

- Full control over API
- Can optimize for exact use case
- Scoping built-in from the start
- Can use link-time backend selection

**Cons:**

- More code to maintain
- Not compatible with `log` ecosystem
- Need to implement all features

#### Option 3: Hybrid - Custom API, `log`-compatible Backend

**Structure:**

```
lp-log/
├── src/
│   ├── lib.rs              # Custom macros with scoping
│   ├── scope.rs            # Scoping logic
│   └── backends/
│       ├── log.rs          # log crate backend (for std)
│       ├── emu.rs          # Syscall backend
│       └── esp32.rs        # ESP32 backend
```

**Pros:**

- Custom API optimized for use case
- Can delegate to `log` crate in std environments
- Full control over scoping
- Can still use `log` ecosystem in std environments

**Cons:**

- More complex implementation
- Need to bridge between custom API and `log`

### Recommended Approach: Option 1 (log + Custom Scoping)

**Rationale:**

1. Leverages standard `log` crate for familiarity and ecosystem compatibility
2. Custom scoping layer can be implemented as a filter/enhancement
3. Backend implementations can be environment-specific
4. Zero-cost when disabled via `log` crate's `max_level_*` features

**Implementation Strategy:**

1. **Public API (`lp-log` crate)**

   ```rust
   // Scoped macros
   lp_debug!("serial", "Got {} bytes", count);
   lp_info!("transport", "Message received");
   lp_warn!("server", "Connection timeout");
   lp_error!("fs", "File not found: {}", path);

   // Unscoped macros (for convenience)
   lp_debug!("Simple message");
   ```

2. **Scoping Logic**
   - Parse `DEBUG` environment variable (comma-separated list)
   - Support wildcards: `DEBUG=*` (all scopes)
   - Case-insensitive matching
   - Check scope before formatting (zero-cost when disabled)

3. **Backend Implementations**
   - **std**: Use `log` crate with custom logger that handles scoping
   - **emu**: Syscall-based backend (`SYSCALL_WRITE` for info/warn/error, `SYSCALL_DEBUG` for debug)
   - **esp32**: ESP32-specific backend (can use `esp_println` or custom serial)

4. **Link-time Backend Selection**
   - Use weak symbols or feature flags
   - For ESP32: Provide `_lplog_write` function that must be linked
   - For emulator: Use syscalls directly
   - For std: Use `log` crate

### Alternative: Simplified Custom Solution

If `log` crate integration proves too complex, a fully custom solution could work:

**API:**

```rust
// Low-level write function (must be provided by backend)
#[linkage = "weak"]
extern "C" {
    fn _lplog_write(level: u8, scope: *const u8, scope_len: usize,
                    msg: *const u8, msg_len: usize);
}

// Macros check scoping and call _lplog_write
lp_debug!("serial", "message");
```

**Pros:**

- Simpler implementation
- Link-time backend selection
- No runtime initialization needed
- Full control

**Cons:**

- Not compatible with `log` ecosystem
- More code to maintain

## Recommendations

### Primary Recommendation: `log` Crate + Custom Scoping

1. **Create `lp-log` crate** with:
   - Scoped macros: `lp_debug!`, `lp_info!`, `lp_warn!`, `lp_error!`
   - Scoping logic that parses `DEBUG` env var
   - Backend trait for different environments

2. **Backend Implementations:**
   - **std backend**: Custom `log::Log` implementation with scoping
   - **emu backend**: Syscall-based (use `SYSCALL_WRITE`/`SYSCALL_DEBUG`)
   - **esp32 backend**: ESP32-specific (provide `_lplog_write` function)

3. **Features:**
   - `std` feature: Enable std backend
   - `emu` feature: Enable emulator backend
   - `max_level_debug`, `max_level_info`, etc.: Zero-cost level filtering

4. **Usage:**

   ```rust
   // In fw-core
   use lp_log::{lp_debug, lp_info};

   lp_debug!("serial", "Got {} bytes", count);
   lp_info!("transport", "Message sent");
   ```

### Alternative: If `log` Integration is Problematic

Use fully custom solution with weak symbol approach:

- Simpler implementation
- Link-time backend selection
- No runtime initialization
- Less ecosystem compatibility

## Open Questions

1. **Should we support `log` crate compatibility?**
   - Pro: Ecosystem compatibility
   - Con: More complex implementation

2. **How should scoping work with levels?**
   - Option A: Scoping only affects `debug` level
   - Option B: Scoping affects all levels (e.g., `INFO=serial`)
   - Recommendation: Start with scoping for `debug` only, can extend later

3. **Should we support structured logging?**
   - For now: No (keep it simple)
   - Future: Could add structured fields if needed

4. **Performance requirements?**
   - Zero-cost when disabled: Yes (via compile-time features)
   - Format string evaluation: Only when scope/level enabled
   - Allocation: Minimize, but may be necessary for formatting

## Next Steps

1. **Decide on approach** (log crate vs custom)
2. **Create `lp-log` crate structure**
3. **Implement scoping logic**
4. **Implement backend trait and std backend**
5. **Implement emulator backend**
6. **Implement ESP32 backend**
7. **Migrate existing code to use new logging system**
8. **Update fw-core to use logging**

## References

- [log crate documentation](https://docs.rs/log/)
- [tracing crate documentation](https://docs.rs/tracing/)
- [defmt documentation](https://defmt.ferrous-systems.com/)
- Current syscall definitions: `lp-riscv-emu-shared/src/syscall.rs`
- Current emulator syscall handling: `lp-riscv-emu/src/emu/emulator/execution.rs`
- ESP32 logging: `lp-fw/fw-esp32/src/main.rs`
