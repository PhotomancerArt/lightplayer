# Phase 2: Output Provider Registry Integration

## Scope Of Phase

Wire the shared hardware registry into the shared output provider path and engine/provider tests.
`MemoryOutputProvider` should claim GPIO plus the single WS281x/RMT resource and release both on
close/drop through existing engine close paths.

Out of scope: ESP32 HAL pin dispatch and `fw-emu` logging provider changes.

## Code Organization Reminders

- Prefer granular files with one main concept per file.
- Keep related output-provider helpers near provider code, but do not put generic hardware model
  code under `output/`.
- Put tests at the bottom of the file.
- Mark any temporary compatibility behavior with a specific TODO and milestone reference.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Update `lp-core/lpc-shared/src/error.rs`:

- Add a hardware-aware `OutputError` variant, preferably
  `Hardware { error: crate::hardware::HardwareError }`.
- Update `fmt::Display` for clear user-facing messages.
- Update `lp-core/lpc-engine/src/engine/error.rs` conversion if needed.

Update `lp-core/lpc-shared/src/output/memory.rs`:

- Replace or augment `open_pins` with a `HardwareRegistry`.
- Preserve `MemoryOutputProvider::new()` by constructing a virtual manifest with GPIO resources and
  one WS281x/RMT resource.
- Add a constructor such as `MemoryOutputProvider::with_hardware_manifest(manifest)` if tests need
  reserved pins or different resources.
- On `open(pin, ...)`, validate byte count and format, build an atomic claim for
  `HardwareAddress::gpio(pin)` plus `HardwareAddress::rmt_ws281x(0)`, and store the lease in
  `ChannelState`.
- On `close(handle)`, release the lease and remove channel state.
- Keep `is_pin_open(pin)` and `get_handle_for_pin(pin)` test helpers working, even if they query the
  registry internally.

Update `lp-core/lpc-engine/src/engine/engine_services.rs` tests:

- Keep the existing drop-close coverage.
- Add a test that two dirty output sinks on the same pin fail gracefully with a duplicate resource
  error and do not leak a partial claim.
- Add a test that two dirty output sinks on different pins contend for the single RMT resource and
  fail with the hardware/RMT address in the error.

Update `lp-core/lpc-engine/src/engine/output_flush_tests.rs` if a full graph-level conflict test is
more appropriate there than in `engine_services.rs`. Prefer one focused service-level test and one
end-to-end output flush test only if both catch distinct behavior.

## Validate

```bash
cargo test -p lpc-shared output
cargo test -p lpc-engine engine_services
cargo test -p lpc-engine output
```
