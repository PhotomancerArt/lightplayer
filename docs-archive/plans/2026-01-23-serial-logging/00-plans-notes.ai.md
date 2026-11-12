# Plan Notes: Migrate esp32-glsl-jit from defmt/RTT to Plain Serial

## Current State

The `esp32-glsl-jit` application currently uses:
- `defmt` for logging with RTT (Real-Time Transfer) backend
- `rtt_target::rtt_init_defmt!()` for initialization
- `panic-rtt-target` for panic handling
- `esp_hal_embassy::main` as the entry point
- `info!` macro from defmt (currently not working)
- `panic!` works but `info!` doesn't

## Target State

Migrate to plain serial logging similar to the embassy_hello_world example:
- Use `esp_println::println!` for logging
- Switch to `#[esp_rtos::main]` instead of `#[esp_hal_embassy::main]`
- Use `esp_rtos::start()` to initialize embassy executor
- Remove defmt dependencies
- Keep embassy executor for async runtime

## Questions

### Q1: Runtime Choice
**Question:** Should we keep using `esp_hal_embassy::main` with embassy executor, or switch to `esp_rtos::main` like the example?

**Context:** 
- Current code uses `esp_hal_embassy::main` and embassy executor
- The embassy_hello_world example uses `esp_rtos::main` 
- Embassy provides async/await runtime which may be needed for other parts of the application

**Suggested Answer:** Keep `esp_hal_embassy::main` since the application already uses embassy executor and async/await patterns. We'll adapt the serial setup to work with embassy.

**Answer:** Switch to `#[esp_rtos::main]` as per embassy_hello_world example.

### Q2: Logging Approach
**Question:** Should we use a simple synchronous `println!` approach or set up async serial tasks like the example?

**Context:**
- The embassy_hello_world example uses `esp_println::println!` directly
- It calls `esp_println::logger::init_logger_from_env()` to initialize logging
- This is simpler than setting up async serial tasks

**Suggested Answer:** Use `esp_println::println!` with `esp_println::logger::init_logger_from_env()` as shown in embassy_hello_world example.

**Answer:** Use `esp_println::println!` approach.

### Q3: Panic Handler
**Question:** What should replace `panic-rtt-target`?

**Context:**
- Current panic handler uses RTT
- Need a panic handler that works with plain serial

**Suggested Answer:** Use `esp_backtrace` which is the standard panic handler for esp-hal and works with serial output.

**Answer:** Use `esp_backtrace` for panic handling.

### Q4: Host Functions
**Question:** How should we update `jit_fns.rs` which currently uses `defmt::debug!` and `defmt::info!`?

**Context:**
- `jit_fns.rs` contains host functions called by JIT-compiled code
- Currently uses defmt macros for output
- Need to replace with serial logging

**Suggested Answer:** Replace defmt macros with `esp_println::println!` or a custom serial write function.

**Answer:** Replace defmt macros with `esp_println::println!`.

### Q5: Dependencies Cleanup
**Question:** Which defmt-related dependencies should be removed?

**Context:**
- Multiple dependencies have `defmt` feature enabled
- Need to remove defmt features and defmt-specific crates
- Keep embassy and other core dependencies

**Suggested Answer:** Remove `defmt` crate, remove `defmt` features from esp-hal, esp-hal-embassy, esp-alloc, embassy-executor, embassy-time. Remove `panic-rtt-target` and `rtt-target` crates. Add `esp-backtrace` for panic handling.

**Answer:** Remove all defmt dependencies and features, follow the embassy_hello_world example. Add esp-backtrace.

## Notes
