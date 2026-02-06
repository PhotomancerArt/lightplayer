# Log Crate Migration - Design

## Scope of Work

Migrate the entire codebase from custom `debug!` macros to the standard `log` crate with `env_logger` for std environments. This plan focuses on infrastructure setup - crate-by-crate migration will be handled separately using IDE find-replace tools.

## File Structure

```
lp-core/lp-shared/src/log/
├── mod.rs                    # (TestLogger already removed by user)
└── ...

Cargo.toml                    # UPDATE: Add test-log dependency to workspace

lp-riscv/lp-riscv-emu-shared/src/
├── syscall.rs                # UPDATE: SYSCALL_DEBUG → SYSCALL_LOG with level support

lp-riscv/lp-riscv-emu-guest/src/
├── host.rs                   # UPDATE: __host_debug → __host_log, remove __host_println
└── log.rs                    # NEW: Logger implementation for emulator guest

lp-riscv/lp-riscv-emu/src/emu/emulator/
├── execution.rs              # UPDATE: Handle SYSCALL_LOG with level filtering
└── logger.rs                 # NEW: Logger that uses env_logger and creates Records from syscalls

lp-fw/fw-core/
├── Cargo.toml                # UPDATE: Add log dependency
├── src/lib.rs                # UPDATE: Export log module
└── src/log/
    ├── mod.rs                # NEW: Logger trait and no_std logger implementation
    ├── emu.rs                # NEW: Emulator backend (uses syscalls)
    └── esp32.rs              # NEW: ESP32 backend (uses esp_println)

lp-fw/fw-emu/src/
├── main.rs                   # UPDATE: Initialize logger
└── log.rs                    # NEW: Logger initialization for emulator guest

lp-fw/fw-esp32/src/
├── main.rs                   # UPDATE: Initialize logger
└── log.rs                    # NEW: Logger initialization for ESP32

lp-glsl/lp-glsl-builtins/src/host/
├── mod.rs                    # UPDATE: Remove host_println, update host_debug → host_log
├── macros.rs                 # UPDATE: Replace host_debug!/host_println! with log macros
├── logger.rs                 # NEW: Logger implementation (routes to emulator/JIT)
├── test.rs                   # UPDATE: Update test implementations
└── no_std_format.rs          # UPDATE: Update for log levels

lp-glsl/lp-glsl-compiler/src/backend/host/
├── impls.rs                  # UPDATE: Replace __host_debug/__host_println with __host_log
└── registry.rs               # UPDATE: Update HostId enum (remove Println, update Debug → Log)

lp-glsl/esp32-glsl-jit/src/
└── jit_fns.rs                # UPDATE: Replace lp_jit_host_debug/lp_jit_host_println with lp_jit_host_log
```

## Conceptual Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    Application Code                          │
│  log::debug!("message")  │  log::info!("message")           │
│  log::warn!("message")   │  log::error!("message")          │
└───────────────────────────┬───────────────────────────────────┘
                            │
                            ▼
┌─────────────────────────────────────────────────────────────┐
│                    log crate (facade)                       │
│  - Macros: debug!, info!, warn!, error!                    │
│  - Global logger registry                                  │
│  - Level filtering (compile-time + runtime)                │
└───────────────────────────┬───────────────────────────────────┘
                            │
                            ▼
        ┌───────────────────┴───────────────────┐
        │                                       │
        ▼                                       ▼
┌──────────────────┐                  ┌──────────────────┐
│   std Context    │                  │   no_std Context │
│  (CLI, Tests,    │                  │  (fw-core,       │
│   Filetests,     │                  │   Emulator       │
│   JIT)           │                  │   Guest, ESP32)  │
└────────┬─────────┘                  └────────┬─────────┘
         │                                     │
         ▼                                     ▼
┌──────────────────┐                  ┌──────────────────┐
│   env_logger     │                  │  Custom Logger  │
│   (reads         │                  │  Implementation │
│   RUST_LOG)      │                  │                  │
└──────────────────┘                  └────────┬─────────┘
                                               │
                    ┌──────────────────────────┼──────────────────────────┐
                    │                          │                          │
                    ▼                          ▼                          ▼
         ┌──────────────────┐      ┌──────────────────┐      ┌──────────────────┐
         │  Emulator Guest │      │  ESP32           │      │  Builtins Host   │
         │  (syscalls)      │      │  (esp_println)   │      │  (syscalls/JIT)  │
         └────────┬─────────┘      └──────────────────┘      └────────┬─────────┘
                  │                                                 │
                  ▼                                                 ▼
         ┌──────────────────┐                          ┌──────────────────┐
         │  SYSCALL_LOG     │                          │  __host_log      │
         │  (guest → host)  │                          │  (builtins)      │
         └────────┬─────────┘                          └────────┬─────────┘
                  │                                            │
                  └──────────────────┬─────────────────────────┘
                                     │
                                     ▼
                          ┌──────────────────┐
                          │  Emulator Host   │
                          │  (env_logger)    │
                          │  (reads RUST_LOG)│
                          └──────────────────┘
```

## Main Components

### 1. Test Logger Setup

**Purpose**: Automatic test logger initialization for all tests

**Implementation**:
- Add `test-log` crate dependency to workspace
- Tests use `#[test_log::test]` instead of `#[test]`
- For tokio tests: `#[tokio::test] #[test_log::test]` (stacked attributes)
- `test-log` automatically initializes `env_logger` before each test
- Reads `RUST_LOG` environment variable for filtering
- No manual initialization needed in any test

**Usage**:
```rust
use test_log::test;

#[test]
fn my_test() {
    log::debug!("This will show if RUST_LOG=debug");
    assert_eq!(2 + 2, 4);
}

#[tokio::test]
#[test_log::test]
async fn my_async_test() {
    log::info!("Async test logging");
}
```

**Dependencies**:
- Add `test-log = "0.2"` to workspace `Cargo.toml` (or individual crate `Cargo.toml` files)
- `test-log` uses `env_logger` under the hood, so `env_logger` must be available

### 2. Emulator Guest Logger (`lp-riscv-emu-guest/src/log.rs`)

**Purpose**: Logger implementation for `no_std` emulator guest code

**Implementation**:
- Implements `log::Log` trait
- Routes all log calls to `SYSCALL_LOG` syscall
- Formats messages with module path, level, and message
- No allocation - uses static buffers for formatting

**Initialization**:
- Called from `fw-emu/src/main.rs` before any logging
- Must be initialized once at startup

### 3. Emulator Host Logger (`lp-riscv-emu/src/emu/emulator/logger.rs`)

**Purpose**: Handle `SYSCALL_LOG` syscalls from guest and route to `env_logger`

**Implementation**:
- Receives syscall with: level, module_path, message
- Creates `log::Record` with guest's module path and level
- Calls `log::log!()` which respects `RUST_LOG` filtering
- Uses `env_logger` for formatting and output

**Integration**:
- Called from `execution.rs` when handling `SYSCALL_LOG` syscall
- Host must initialize `env_logger` before running guest code

### 4. ESP32 Logger (`lp-fw/fw-core/src/log/esp32.rs`)

**Purpose**: Logger implementation for ESP32 `no_std` environment

**Implementation**:
- Implements `log::Log` trait
- Routes to `esp_println::println!` with formatted output
- Filters at `info` level by default (hardcoded)
- Formats: `[LEVEL] module::path: message`

**Initialization**:
- Called from `fw-esp32/src/main.rs` at startup
- Can be initialized with custom level if needed (future)

### 5. Builtins Host Logger (`lp-glsl-builtins/src/host/logger.rs`)

**Purpose**: Logger implementation for GLSL builtins (works in both emulator and JIT)

**Implementation**:
- Implements `log::Log` trait
- Routes to `__host_log` function (extern "C")
- In emulator: `__host_log` uses `SYSCALL_LOG`
- In JIT: `__host_log` uses `log` crate directly (std context)
- In tests: `__host_log` uses `log` crate directly

**Initialization**:
- Called from GLSL compiler or emulator guest code
- Must be initialized before GLSL code runs

### 6. Syscall Refactoring

**SYSCALL_LOG** (replaces `SYSCALL_DEBUG`):
- `args[0]`: level (u8: 0=error, 1=warn, 2=info, 3=debug)
- `args[1]`: module_path pointer (as i32)
- `args[2]`: module_path length (as i32)
- `args[3]`: message pointer (as i32)
- `args[4]`: message length (as i32)

**__host_log** (replaces `__host_debug`):
- Same signature as `SYSCALL_LOG`
- In emulator: delegates to `SYSCALL_LOG`
- In JIT: creates `log::Record` and calls `log::log!()`
- In tests: creates `log::Record` and calls `log::log!()`

## Component Interactions

### Std Applications (CLI, Filetests)

1. Application calls `env_logger::init()` or `env_logger::try_init()`
2. Reads `RUST_LOG` environment variable
3. All `log::debug!()` etc. calls are filtered and formatted by `env_logger`
4. Output goes to stderr

### Tests

1. Test uses `#[test_log::test]` attribute (or `#[tokio::test] #[test_log::test]` for async)
2. `test-log` automatically initializes `env_logger` before test runs
3. Reads `RUST_LOG` environment variable for filtering
4. All log calls filtered and formatted
5. Output goes to stderr (doesn't interfere with test output)
6. No manual initialization needed - works automatically

### Emulator Guest (fw-emu)

1. Guest code calls `log::debug!()` etc.
2. `fw-core` logger implementation routes to `SYSCALL_LOG` syscall
3. Syscall includes: level, module_path, message
4. Host receives syscall in `execution.rs`
5. Host creates `log::Record` and calls `log::log!()`
6. Host's `env_logger` filters based on `RUST_LOG`
7. If enabled, formatted output goes to stderr

### ESP32

1. ESP32 code calls `log::debug!()` etc.
2. `fw-core` ESP32 logger implementation filters at `info` level
3. Formats message and calls `esp_println::println!()`
4. Output goes to serial port

### GLSL Builtins (Emulator)

1. GLSL code calls `log::debug!()` etc. (via builtins logger)
2. Builtins logger routes to `__host_log`
3. `__host_log` in guest uses `SYSCALL_LOG` syscall
4. Same flow as emulator guest above

### GLSL Builtins (JIT)

1. GLSL code calls `log::debug!()` etc. (via builtins logger)
2. Builtins logger routes to `__host_log`
3. `__host_log` in JIT creates `log::Record` directly
4. Calls `log::log!()` which uses JIT's logger (likely `env_logger`)
5. Filtered and formatted by `env_logger`

## Migration Strategy

### Phase 1: Infrastructure Setup (This Plan)

1. Add `test-log` crate dependency to workspace
2. Refactor syscalls (`SYSCALL_DEBUG` → `SYSCALL_LOG`)
3. Create emulator guest logger
4. Create emulator host logger
5. Create ESP32 logger
6. Create builtins host logger
7. Refactor `__host_debug` → `__host_log`
8. Remove `__host_println` and `host_println!`
9. Add logging to `fw-core` with a few example logs
10. Update all logger initializations

### Phase 2: Crate Migration (User Handled)

User will migrate crates one by one using IDE find-replace:
- Replace `crate::debug!(...)` with `log::debug!(...)`
- Replace `host_debug!(...)` with `log::debug!(...)`
- Replace `host_println!(...)` with `log::info!(...)`
- Remove old macro definitions

### Phase 3: Cleanup

- Remove all old `debug!` macro definitions
- Remove `host_debug!` and `host_println!` macros
- Update documentation
- Verify all tests pass

## Key Design Decisions

1. **Use `log` crate everywhere**: Provides consistent API across all environments
2. **Use `env_logger` for std**: Standard, well-tested, supports `RUST_LOG`
3. **Custom loggers for no_std**: Route to appropriate backends (syscalls, esp_println)
4. **Module paths for scoping**: Auto-generated, works with `RUST_LOG=module::path=level`
5. **Automatic test initialization**: Use `test-log` crate - no manual initialization needed
6. **Syscall-based for emulator**: Guest uses syscalls, host uses `env_logger`
7. **Hardcoded level for ESP32**: Default to `info`, can be enhanced later
8. **Remove println in favor of logging**: Consistent logging levels everywhere
