# Log Crate Migration Plan - Notes

## Scope of Work

Migrate the entire codebase from custom `debug!` macros to the standard `log` crate with `env_logger` for std environments. This includes:

1. **Std Applications**: Use `env_logger` with `RUST_LOG` environment variable
   - CLI (`lp-cli`)
   - Filetest runner (`lp-glsl-filetests`)
   - Other std binaries

2. **Tests**: Use `env_logger` in tests (with proper feature flag handling)
   - All test suites should be able to use `RUST_LOG` for filtering
   - Tests run with `std` feature enabled

3. **Emulator Guest**: Create adapter that delegates to host via syscalls
   - `fw-emu` runs in `no_std` environment
   - Uses syscalls (`SYSCALL_WRITE`, `SYSCALL_DEBUG`) to communicate with host
   - Host side (emulator) should use `env_logger` to handle the messages

4. **ESP32**: Custom logger implementation
   - `fw-esp32` runs in `no_std` environment
   - Should use `esp_println` or similar for output
   - Can read `RUST_LOG` at compile-time or use defaults

5. **Migration**: Replace all custom `debug!` macros with `log::debug!`
   - Remove custom debug macro definitions
   - Update all usages to use `log` crate macros
   - Ensure module paths are used correctly for scoping

## Current State

### Custom Debug Macros

1. **lp-glsl-compiler/src/debug.rs**
   - Checks `DEBUG=1` env var
   - Only works with `std` feature
   - Used extensively throughout compiler code

2. **lp-riscv-inst/src/debug.rs**
   - Similar pattern: checks `DEBUG=1`
   - Used in instruction-related code

3. **lp-riscv-elf/src/lib.rs**
   - Inline debug macro definition
   - Checks `DEBUG=1`

4. **lp-glsl-builtins/src/host/macros.rs**
   - `host_debug!` macro for emulator guest
   - Uses syscalls in no_std mode
   - Has complex feature flag handling

### Existing Log Infrastructure

1. **lp-shared/src/log/mod.rs**
   - Already uses `log` crate
   - Has `TestLogger` implementation
   - Only used in tests currently
   - Comment notes: "Firmware is responsible for initializing the logger"

2. **lp-core/lp-shared/Cargo.toml**
   - Already has `log = { version = "0.4", default-features = false }`

### Environment-Specific Code

1. **fw-emu** (`lp-fw/fw-emu`)
   - Uses `host_debug!` from `lp-glsl-builtins`
   - Runs in `no_std` environment
   - Syscalls available: `SYSCALL_WRITE`, `SYSCALL_DEBUG`

2. **fw-esp32** (`lp-fw/fw-esp32`)
   - Uses `esp_println::println!` directly
   - Calls `esp_println::logger::init_logger_from_env()`
   - Runs in `no_std` environment

3. **Std Applications**
   - CLI, filetest runner, etc.
   - Can use `env_logger` directly

### Usage Patterns

- `crate::debug!("message")` - Used in lp-glsl-compiler extensively
- `host_debug!("message")` - Used in emulator guest code
- Direct `println!` - Used in ESP32 and some std code

## Questions

1. **Test Feature Flags**: ✅ RESOLVED - Use `test-log` crate for automatic test logger initialization. Tests use `#[test_log::test]` instead of `#[test]`, and `#[tokio::test] #[test_log::test]` for async tests. This automatically initializes `env_logger` before each test, reading `RUST_LOG` for filtering. No manual initialization needed.

2. **Module Path Scoping**: ✅ RESOLVED - Use auto-generated module paths. Replace `crate::debug!("message")` with `log::debug!("message")` and let the log crate capture module paths automatically. Users can filter with `RUST_LOG=fw_core::transport::serial=debug` or `RUST_LOG=fw_core=debug`.

3. **Emulator Host Side**: ✅ RESOLVED - Refactor `SYSCALL_DEBUG` to `SYSCALL_LOG` to support all log levels (debug, info, warn, error). Emulator host should use `env_logger` and respect `RUST_LOG`. When guest sends a log message via syscall, host creates a `log::Record` with the guest's module path and level, then calls `log::log!()` which respects `RUST_LOG` filtering. This means guest messages are filtered by the host's `RUST_LOG` setting.

4. **ESP32 RUST_LOG**: ✅ RESOLVED - Hardcode default to `info` level for now. ESP32 logger implementation will filter logs at info level and above (info, warn, error). Defer compile-time feature flags for debug builds to later.

5. **Migration Strategy**: ✅ RESOLVED - Hybrid approach: Phase 1 focuses on infrastructure setup (test logger, emulator logger, ESP32 logger, syscall refactor). Once infrastructure is complete, user will handle crate-by-crate migration using IDE find-replace tools. Plan will provide clear migration guidance.

6. **Backward Compatibility**: ✅ RESOLVED - Keep old `debug!` macros working during infrastructure setup. Remove them once all crates are migrated. This avoids breaking builds during infrastructure work.

7. **fw-core Logging**: ✅ RESOLVED - Set up infrastructure: add `log` crate dependency, create logger implementation for no_std. Add a few basic debug logs in `SerialTransport` to verify it works. More logging calls can be added incrementally later.

8. **host_debug! Macro**: ✅ RESOLVED - Refactor `host_debug!` and `host_println!` to use logging system:
   - Replace `__host_debug` with `__host_log` (supports all log levels: error, warn, info, debug)
   - Remove `__host_println` - use logging system instead (log::info! for what was println)
   - Support both emulator and JIT contexts:
     - Emulator: `__host_log` uses `SYSCALL_LOG` syscall (no_std)
     - JIT: `__host_log` uses `log` crate directly (std context)
     - Tests: Use `log` crate directly (std context)
   - Create a logger implementation in `lp-glsl-builtins` that routes to appropriate backend
   - Signature: `__host_log(level: u8, module_path_ptr: *const u8, module_path_len: usize, msg_ptr: *const u8, msg_len: usize)`

## Notes

- The `log` crate supports `no_std` with `default-features = false`
- `env_logger` requires `std` but is perfect for std applications and tests
- Module paths work well for scoping with `RUST_LOG=module::path=level`
- Tests can use `env_logger::try_init()` to avoid panics if logger already set
- ESP32's `esp_println::logger::init_logger_from_env()` already reads `RUST_LOG`!
- Use `test-log` crate for automatic test logger initialization - tests use `#[test_log::test]` attribute
- `test-log` works with both regular and tokio tests by stacking attributes
- Refactor `SYSCALL_DEBUG` to `SYSCALL_LOG` with level parameter: args[0] = level (u8: 0=error, 1=warn, 2=info, 3=debug), args[1] = module_path ptr, args[2] = module_path len, args[3] = message ptr, args[4] = message len
- Refactor `__host_debug` to `__host_log` in `lp-glsl-builtins` with same signature as SYSCALL_LOG
- Remove `__host_println` and `host_println!` macro - replace usages with `log::info!` or appropriate level
