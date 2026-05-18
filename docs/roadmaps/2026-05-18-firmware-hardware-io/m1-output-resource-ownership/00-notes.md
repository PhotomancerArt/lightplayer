# M1 Output Resource Ownership Notes

## Scope

Milestone 1 implements the first small hardware ownership spine for output devices. It adds shared
`no_std + alloc` hardware vocabulary, an in-memory registry, ESP32-C6 board metadata, and output
provider integration so WS281x outputs claim both their GPIO and the currently single RMT resource.

This plan intentionally does not add GPIO input, radio transport, multi-RMT output, project-level
pin selection UI, or dynamic board config loading. Authored output nodes keep `pin = 18` as the
compatibility surface for M1.

## Current State

- `lp-core/lpc-model/src/nodes/output/output_def.rs` stores `OutputDef.pin` as
  `ValueSlot<u32>`, exposes `OutputDef::pin()`, and has TOML coverage for flat `pin = 18`.
- `lp-core/lpc-engine/src/engine/engine_services.rs` registers output sinks from
  `OutputDef::pin()` and opens provider channels lazily during dirty-sink flush.
- `lp-core/lpc-shared/src/output/provider.rs` defines `OutputProvider::open(pin, byte_count,
  format, options)`. The trait is already shared by engine, server, `fw-esp32`, `fw-emu`, and tests.
- `lp-core/lpc-shared/src/output/memory.rs` prevents duplicate numeric pins with an `open_pins`
  set, but it has no concept of RMT contention, reserved pins, or non-GPIO resources.
- `lp-core/lpc-shared/src/error.rs` has `OutputError::PinAlreadyOpen`, `InvalidConfig`,
  `InvalidHandle`, `DataLengthMismatch`, and `Other`. Hardware-specific conflict errors do not
  exist yet.
- `lp-fw/fw-esp32/src/output/provider.rs` keeps channel state and duplicate-pin tracking, while the
  actual `LedChannel` lives in `static mut LED_CHANNEL`. `Esp32OutputProvider::init_rmt(...)`
  initializes one channel at boot using GPIO18.
- `lp-fw/fw-esp32/src/main.rs` initializes RMT at boot, creates `Esp32OutputProvider`, calls
  `Esp32OutputProvider::init_rmt(rmt, gpio18, 256)`, then hands the provider to `lpa-server`.
- `lp-fw/fw-esp32/src/board/esp32c6/init.rs` currently returns concrete peripherals including
  `RMT`, `USB_DEVICE`, `GPIO18`, `FLASH`, `GPIO4`, and `WIFI`.
- `lp-fw/fw-esp32/src/tests/test_gpio.rs` manually scans GPIO 0-21 and excludes GPIO12 because it
  has been observed to crash the device.
- `lp-fw/fw-emu/src/output.rs` logs open/write/close calls through the emulator and does not
  currently enforce pin or RMT conflicts.

## User And Roadmap Notes

- The on-device GLSL JIT compiler is the core product. M1 must avoid feature-gating or disturbing
  the shader compile/execute path.
- The registry owns metadata and runtime claims; drivers consume leases and HAL resources.
- Claims are atomic bundles so a WS281x output gets GPIO plus RMT or gets nothing.
- Hardware addresses should normalize toward string-capable names like `"/gpio/18"` while keeping
  numeric output pins as compatibility input.
- Initial shared hardware types should live in `lpc-shared::hardware`.
- `fw-emu` should use the same shared ownership behavior where practical.
- M1 needs clear errors for duplicate pins, reserved pins, unsupported capabilities, and RMT
  contention.
- Board silkscreen labels may not match HAL GPIO numbers, and different ESP32-C6 dev boards may use
  different labels for the same HAL GPIO. UI-facing code should show the label printed on the user's
  board, while internal claims should use stable HAL/resource addresses such as `"/gpio/18"`.
- Manifest calibration is parked in M1.1. M1 should add fields that make calibration possible, but
  should not build the firmware/CLI calibration workflow.

## Open Questions

### Should M1 change authored output from `pin = 18` to hardware addresses?

Suggested answer: no. Keep `OutputDef.pin: ValueSlot<u32>` and normalize to `HardwareAddress` inside
the output/provider boundary. This preserves existing authored files while establishing the internal
address model.

Status: resolved by roadmap decision.

### Should `OutputProvider::open` change its public signature?

Suggested answer: no for M1. Keep the trait signature stable and let providers normalize the numeric
pin into a hardware claim internally. Add hardware-aware helper types and errors beneath the existing
trait. A later model milestone can introduce an address/string slot or driver table.

Status: plan assumption.

### Should the memory provider model RMT contention?

Suggested answer: yes, when constructed with a hardware registry/manifest. Preserve
`MemoryOutputProvider::new()` compatibility by making it create a virtual board with GPIO resources
and one WS281x/RMT resource by default.

Status: plan assumption.

### How far should ESP32 dynamic pin selection go in M1?

Suggested answer: add the board manifest and a small dispatch/ownership shape, but only require the
known GPIO18 RMT path to transmit on hardware. Requests for other manifest-valid pins should fail
clearly until the HAL peripheral dispatch table is added in the same or a follow-up slice if it is
too broad. The product-critical part of M1 is safe resource ownership and clear failure, not full
multi-pin LED output.

Status: plan assumption. Phase 3 calls out this risk explicitly.

### How should board silkscreen labels relate to internal GPIO addresses?

Suggested answer: keep internal identity as stable hardware addresses such as `"/gpio/18"` and add
board-profile metadata for user-facing labels. A `HardwareResource` should be able to carry a
primary display label, optional aliases, and optional physical location notes. The UI should show the
selected board profile's label, but persisted/project/runtime claims should continue to use the
stable internal address.

Status: user clarified after initial plan; incorporate into M1 metadata.

### Should M1 build the board calibration app?

Suggested answer: no. M1 only needs static/provisional board-profile labels and reserved-pin notes.
The interactive firmware test mode and `lp-cli` workflow belong to
`docs/roadmaps/2026-05-18-firmware-hardware-io/m1.1-board-manifest-calibration/00-notes.md`.

Status: user requested M1.1 notes and a return to M1.

## References

- `docs/roadmaps/2026-05-18-firmware-hardware-io/overview.md`
- `docs/roadmaps/2026-05-18-firmware-hardware-io/m1-output-resource-ownership.md`
- `docs/roadmaps/2026-05-18-firmware-hardware-io/decisions.md`
- `lp-core/lpc-shared/src/output/provider.rs`
- `lp-core/lpc-shared/src/output/memory.rs`
- `lp-core/lpc-engine/src/engine/engine_services.rs`
- `lp-core/lpc-engine/src/engine/output_flush_tests.rs`
- `lp-fw/fw-esp32/src/output/provider.rs`
- `lp-fw/fw-esp32/src/board/esp32c6/init.rs`
- `lp-fw/fw-emu/src/output.rs`
