# Phase 1: Update Cargo.toml Dependencies

## Description

Remove all defmt-related dependencies and features, and add the new dependencies required for plain serial logging with esp_rtos.

## Changes

### Remove Dependencies
- Remove `defmt = "1.0.1"`
- Remove `panic-rtt-target`
- Remove `rtt-target`

### Remove defmt Features
Remove `defmt` feature from:
- `esp-hal` (change from `features = ["defmt", "esp32c6", "unstable"]` to `features = ["esp32c6", "unstable"]`)
- `esp-hal-embassy` (remove entirely if not needed, or remove defmt feature)
- `esp-alloc` (remove defmt feature, or remove if not needed)
- `embassy-executor` (remove defmt feature)
- `embassy-time` (remove defmt feature)

### Add Dependencies
- Add `esp-backtrace` with features `["panic-handler", "println"]`
- Add `esp-rtos` with features `["embassy", "log-04"]`
- Add `esp-println` with features `["log-04"]`

### Version Updates
- Check and update `embassy-executor` version if needed (example uses 0.9.0, current is 0.7.0)
- Check and update `embassy-time` version if needed (example uses 0.5.0, current is 0.4.0)

## Success Criteria

- Cargo.toml compiles without errors
- All defmt references removed
- New dependencies added with correct features
- Version compatibility verified

## Code Organization

- Place dependency changes in logical groups (remove, add, modify)
- Keep comments explaining changes if needed

## Formatting

- Run `cargo +nightly fmt` on Cargo.toml (if supported) or ensure proper formatting

## Language and Tone

- Use measured, factual descriptions
- Avoid overly optimistic language
