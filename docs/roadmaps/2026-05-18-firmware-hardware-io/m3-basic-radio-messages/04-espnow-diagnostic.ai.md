# Phase 4: Production-Backed ESP-NOW Diagnostic

## Scope Of Phase

In scope:

- Update `test_espnow` to use the production radio message/driver code.
- Preserve the useful current diagnostic behavior: simulated periodic button event, broadcast, receive, duplicate logs.
- Keep this as a firmware diagnostic, not a node/runtime feature.

Out of scope:

- Normal firmware button-to-radio wiring.
- Radio node implementation.
- Host integration tests that require physical ESP32 boards.

## Code Organization Reminders

- Keep diagnostic-only loops in `tests/test_espnow.rs`.
- Do not duplicate packet encode/decode logic in the diagnostic.
- If a tiny diagnostic helper is reusable by normal boot, put it in the driver module rather than the test file.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-fw/fw-esp32/src/tests/test_espnow.rs`
- `lp-fw/fw-esp32/src/hardware/espnow_radio_driver.rs`
- `lp-fw/fw-esp32/src/hardware/espnow_radio_task.rs`
- `lp-fw/fw-esp32/Cargo.toml`

Expected changes:

- Change `test_espnow` to depend on the `radio` feature.
- Initialize the production ESP-NOW radio driver instead of direct `esp_radio::wifi::new(...).esp_now()` calls in the test file.
- Open `/radio/0`, subscribe to a diagnostic channel, and send `RadioMessageKind::ButtonPress` once per second.
- Drain the diagnostic channel and log source device ID, event ID, duplicate/drop state, and payload summary.
- Remove the local `ButtonEvent`, packet magic constants, and `SeenRing` from `test_espnow.rs` if those are now owned by shared/driver code.

Tests/checks:

- Compile `test_espnow`.
- If possible, add a host/shared test proving duplicate filtering is applied by the worker-support data structure without hardware.

## Validate

```bash
cargo fmt --check
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,test_espnow
```
