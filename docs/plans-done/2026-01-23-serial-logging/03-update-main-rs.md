# Phase 3: Update main.rs

## Description

Update `main.rs` to switch from `esp_hal_embassy::main` to `esp_rtos::main`, replace defmt logging with esp_println, and update the initialization sequence.

## Changes

### Imports
- **REMOVE:** `use defmt::info;`
- **REMOVE:** `use panic_rtt_target as _;`
- **ADD:** `use esp_backtrace as _;`
- **ADD:** `use esp_println::println;`
- **ADD:** `use esp_hal::{interrupt::software::SoftwareInterruptControl, timer::timg::TimerGroup};`
- **CHANGE:** Update `esp_hal` imports to remove `timer::systimer::SystemTimer` if not needed

### Entry Point
- **CHANGE:** `#[esp_hal_embassy::main]` → `#[esp_rtos::main]`

### Initialization
- **REMOVE:** `esp_hal_embassy::init(timer0.alarm0);`
- **REMOVE:** `rtt_target::rtt_init_defmt!();`
- **ADD:** `esp_println::logger::init_logger_from_env();`
- **ADD:** `let sw_int = SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);`
- **ADD:** `let timg0 = TimerGroup::new(peripherals.TIMG0);`
- **ADD:** `esp_rtos::start(timg0.timer0, sw_int.software_interrupt0);`

### Logging
- **CHANGE:** All `info!(...)` calls → `println!(...)`
- **CHANGE:** All `defmt::panic!(...)` calls → `panic!(...)`

## Success Criteria

- Code compiles without errors
- All defmt references removed
- esp_rtos initialization working
- Logging uses esp_println

## Code Organization

- Place helper utility functions at the bottom of files
- Place more abstract things, entry points, and tests first
- Keep related functionality grouped together

## Formatting

- Run `cargo +nightly fmt` on all changes before committing

## Language and Tone

- Keep language professional and restrained
- Avoid overly optimistic language
- Use measured, factual descriptions
