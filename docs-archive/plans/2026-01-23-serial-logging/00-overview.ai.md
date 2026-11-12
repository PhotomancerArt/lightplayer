# Plan: Migrate esp32-glsl-jit from defmt/RTT to Plain Serial

## Overview

Migrate the `esp32-glsl-jit` application from defmt/RTT logging to plain serial logging using `esp_println`, following the `embassy_hello_world` example pattern. This will fix the non-working `info!` macro and simplify the logging infrastructure.

## Phases

1. Update Cargo.toml dependencies
2. Create .cargo/config.toml
3. Update main.rs to use esp_rtos::main and esp_println
4. Update jit_fns.rs to use esp_println
5. Cleanup and finalization

## Success Criteria

- Code compiles without errors
- All defmt dependencies removed
- Logging works via serial using esp_println
- Panic handler uses esp-backtrace
- Cargo runner configured for espflash
- All tests pass (if any)
- Code formatted with `cargo +nightly fmt`
