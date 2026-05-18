# M1 Output Resource Ownership Design

## Scope Of Work

M1 adds a small hardware registry and wires WS281x output opening through it. Output opens claim
GPIO plus RMT as an atomic bundle; close/drop releases the lease. The shared model must stay
`no_std + alloc`, work for host tests and `fw-emu`, and preserve existing authored `pin = 18`
behavior.

Out of scope: GPIO input, radio service integration, full board database, UI pin pickers, dynamic
board config files, and multi-RMT-channel LED output.

## File Structure

```text
lp-core/lpc-shared/src/
  lib.rs
  error.rs
  hardware/
    mod.rs
    hardware_address.rs
    hardware_capability.rs
    hardware_manifest.rs
    hardware_registry.rs
    hardware_resource.rs
    hardware_claim.rs
    hardware_lease.rs
    hardware_error.rs
  output/
    memory.rs
    provider.rs

lp-core/lpc-engine/src/engine/
  engine_services.rs
  output_flush_tests.rs

lp-fw/fw-esp32/src/
  board/esp32c6/
    init.rs
    hardware_manifest.rs
    gpio_output_table.rs
  output/
    provider.rs

lp-fw/fw-emu/src/
  output.rs
```

## Architecture Summary

`lpc_shared::hardware` owns the shared resource vocabulary:

- `HardwareAddress` normalizes paths such as `"/gpio/18"` and `"/rmt/ws281x0"`.
- `HardwareCapability` describes coarse uses such as GPIO output, GPIO input, WS281x output, RMT,
  and radio.
- `HardwareResource` and `HardwareManifest` describe what a board exposes, what is reserved, and
  what user-facing board labels/aliases map to each internal address.
- `HardwareClaim` is a bundle of required resources.
- `HardwareRegistry` tracks active claims and returns `HardwareLease` values.
- `HardwareError` reports unavailable, reserved, unsupported, already claimed, and invalid address
  cases.

The registry is not a driver manager. Drivers and providers ask the registry for a lease before they
touch HAL resources. Lease drop should release claims where possible, but providers should still
close explicitly so existing error behavior remains predictable.

## Main Components And Interactions

1. `OutputProvider::open(pin, ...)` remains the public compatibility entry point for M1.
2. Providers convert `pin` to `HardwareAddress::gpio(pin)` and build a WS281x claim containing:
   the GPIO resource and the board's single WS281x/RMT resource.
3. `HardwareRegistry::claim_bundle(...)` validates all resources before mutating state. If any
   resource is reserved, missing, unsupported, or already claimed, no partial claim remains.
4. Provider channel state stores the returned `HardwareLease` alongside the existing output handle,
   byte count, format, and display pipeline.
5. Provider close removes the channel and releases the lease. Existing `EngineServices::Drop`
   continues to close open output handles, which releases hardware resources.
6. `MemoryOutputProvider` and `fw-emu::SyscallOutputProvider` use the same registry semantics so
   host and emulator behavior match ESP32 conflict behavior for duplicate pins and RMT contention.
7. `fw-esp32` adds an ESP32-C6 manifest and reserves known-dangerous/off-limits pins, especially
   GPIO12. The first implementation may keep actual transmission on GPIO18 only, but any unsupported
   dynamic pin path must fail with a clear provider error after successful registry validation is
   rolled back.

## Board Labels And Identity

Internal resource identity should be HAL/resource based: `"/gpio/18"` means ESP HAL GPIO18, not
whatever a particular dev board prints next to a header pin. Board silkscreen belongs in metadata.

Each GPIO `HardwareResource` should include:

- stable `address`: `"/gpio/18"`
- `display_label`: the board-profile-specific label a user sees on the board, such as `"D6"` or
  `"GPIO18"`
- optional `aliases`: alternate printed/common names when useful
- optional `location`: short physical hint such as `"left header, pin 7"` if known
- optional `reserved_reason`: why the UI should hide or disable the resource

This lets project/runtime code claim stable addresses while UI-facing code can show the selected
board profile's silkscreen. Different ESP32-C6 dev boards should be separate board profiles or
manifest constructors even if they share the same chip.

M1 may use provisional hand-entered board labels. The interactive calibration workflow that pulses
GPIOs, handles device resets, and writes measured label mappings is M1.1, documented in
`docs/roadmaps/2026-05-18-firmware-hardware-io/m1.1-board-manifest-calibration/00-notes.md`.

## Error Mapping

Add `OutputError::Hardware { error: HardwareError }` or an equivalent variant in
`lp-core/lpc-shared/src/error.rs`. Keep `PinAlreadyOpen` usable for compatibility only if callers
still match it directly; otherwise convert duplicate GPIO claims to the hardware error and update
engine error conversion.

Hardware errors should include enough address/resource context for user-facing project diagnostics:
reserved GPIO, resource already claimed, unknown GPIO, unsupported output capability, and RMT
contention.

## Validation Strategy

Each phase has targeted host tests. Final validation should run:

```bash
cargo test -p lpc-shared
cargo test -p lpc-engine output
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server
cargo check -p fw-emu --target riscv32imac-unknown-none-elf --profile release-emu
cargo check -p lpa-server
cargo test -p lpa-server --no-run
```

If the shader pipeline is touched unexpectedly, also run the roadmap shader-pipeline validation
commands from `AGENTS.md`.
