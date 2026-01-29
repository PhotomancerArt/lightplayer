# Design: Migrate esp32-glsl-jit from defmt/RTT to Plain Serial

## Overview

Migrate the `esp32-glsl-jit` application from defmt/RTT logging to plain serial logging using `esp_println`, following the `embassy_hello_world` example pattern. This will fix the non-working `info!` macro and simplify the logging infrastructure.

## Architecture Changes

### Entry Point
- **Change:** Switch from `#[esp_hal_embassy::main]` to `#[esp_rtos::main]`
- **Reason:** Follow embassy_hello_world example pattern
- **Impact:** Requires different initialization sequence

### Initialization Sequence
- **Remove:** `esp_hal_embassy::init()` and `rtt_target::rtt_init_defmt!()`
- **Add:** `esp_println::logger::init_logger_from_env()` and `esp_rtos::start()`
- **Change:** Use `TimerGroup` and `SoftwareInterruptControl` for esp_rtos setup

### Logging
- **Remove:** `defmt::info!`, `defmt::debug!`, `defmt::panic!`
- **Add:** `esp_println::println!` for all logging
- **Change:** Host functions in `jit_fns.rs` use `esp_println::println!`

### Panic Handler
- **Remove:** `panic-rtt-target`
- **Add:** `esp-backtrace` with `panic-handler` and `println` features

## File Structure

```
lp-glsl/apps/esp32-glsl-jit/
├── .cargo/
│   └── config.toml              # NEW: Cargo config for espflash runner and link args
├── Cargo.toml                    # UPDATE: Remove defmt deps, add esp-rtos, esp-println, esp-backtrace
└── src/
    ├── main.rs                   # UPDATE: Switch to esp_rtos::main, use esp_println
    └── jit_fns.rs                # UPDATE: Replace defmt macros with esp_println
```

## Type and Function Changes

### main.rs
- **REMOVE:** `use defmt::info;`
- **REMOVE:** `use panic_rtt_target as _;`
- **ADD:** `use esp_backtrace as _;`
- **ADD:** `use esp_println::println;`
- **ADD:** `use esp_hal::{interrupt::software::SoftwareInterruptControl, timer::timg::TimerGroup};`
- **ADD:** `use esp_rtos;`
- **CHANGE:** `#[esp_hal_embassy::main]` → `#[esp_rtos::main]`
- **CHANGE:** Initialization sequence to use `esp_rtos::start()` instead of `esp_hal_embassy::init()`
- **CHANGE:** All `info!()` calls → `println!()` calls
- **CHANGE:** All `defmt::panic!()` calls → `panic!()` calls

### jit_fns.rs
- **REMOVE:** `use defmt::debug;` and `use defmt::info;`
- **ADD:** `use esp_println::println;`
- **CHANGE:** `defmt::debug!()` → `println!()`
- **CHANGE:** `defmt::info!()` → `println!()`

### Cargo.toml
- **REMOVE:** `defmt = "1.0.1"`
- **REMOVE:** `panic-rtt-target`
- **REMOVE:** `rtt-target`
- **REMOVE:** `defmt` feature from:
  - `esp-hal`
  - `esp-hal-embassy` (or remove entirely if not needed)
  - `esp-alloc`
  - `embassy-executor`
  - `embassy-time`
- **ADD:** `esp-backtrace = { version = "...", features = ["panic-handler", "println"] }`
- **ADD:** `esp-rtos = { version = "...", features = ["embassy", "log-04"] }`
- **ADD:** `esp-println = { version = "...", features = ["log-04"] }`
- **CHANGE:** `esp-hal` features: remove `defmt`, add `log-04` if needed
- **CHANGE:** `embassy-executor` version may need update (example uses 0.9.0)
- **CHANGE:** `embassy-time` version may need update (example uses 0.5.0)

## Dependencies Summary

### To Remove
- `defmt`
- `panic-rtt-target`
- `rtt-target`
- `defmt` feature from all dependencies

### To Add
- `esp-backtrace` (with `panic-handler` and `println` features)
- `esp-rtos` (with `embassy` and `log-04` features)
- `esp-println` (with `log-04` feature)

### To Modify
- `esp-hal`: Remove `defmt` feature, potentially add `log-04`
- `embassy-executor`: Remove `defmt` feature
- `embassy-time`: Remove `defmt` feature
- `esp-alloc`: Remove `defmt` feature (or remove if not needed)
- `esp-hal-embassy`: Remove `defmt` feature (or remove if not needed with esp-rtos)

## Cargo Configuration

Create `.cargo/config.toml` with:
- RISC-V32 target configuration for espflash runner
- Linker flags for `linkall.x`
- Force frame pointers for better debugging
- ESP_LOG environment variable set to "info"
- Build std configuration for no_std targets

## Implementation Notes

1. The `esp_rtos::start()` function requires a timer and software interrupt control
2. `esp_println::logger::init_logger_from_env()` should be called early in main
3. All logging calls need to be updated from defmt macros to `println!`
4. Panic messages will now go through esp-backtrace to serial
5. Version compatibility: Check that embassy versions match what's available
6. Create `.cargo/config.toml` for proper espflash runner configuration
