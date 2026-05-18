# Phase 3: ESP32 Board And Output Provider

## Scope Of Phase

Add ESP32-C6 board hardware metadata and integrate `Esp32OutputProvider` with the hardware registry.
The provider must claim GPIO plus RMT before opening a WS281x output, release the lease on close, and
report reserved/unsupported/contentious resources clearly.

Out of scope: GPIO input, radio support, multiple RMT channels, and full dynamic LED output on every
manifest GPIO if HAL ownership makes that too large for M1.

## Code Organization Reminders

- Prefer granular files with one main concept per file.
- Keep board metadata under `board/esp32c6/`; keep RMT/WS281x transmission in `output/`.
- Avoid turning the registry into a driver registry.
- Put tests at the bottom of files when host-testable; firmware-only smoke paths can stay behind
  existing feature flags.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Add `lp-fw/fw-esp32/src/board/esp32c6/hardware_manifest.rs`:

- Build an ESP32-C6 manifest with GPIO resources for known pins and a single WS281x/RMT resource.
- Include board-profile-specific `display_label`/alias metadata for each GPIO so UI code can show
  the silkscreen label the user sees while providers keep using internal `"/gpio/X"` addresses.
- Start with the actual board profile in hand. If there are two known ESP32-C6 dev boards with
  different silkscreen, add separate constructors or clearly named profile data for both instead of
  pretending there is one universal ESP32-C6 label map.
- Use provisional/manual label data in M1. Do not implement the firmware square-wave calibration
  mode or `lp-cli` calibration workflow here; those are M1.1.
- Mark GPIO12 reserved/dangerous with a reason from the existing GPIO test note.
- Mark USB/JTAG/flash/boot-sensitive pins conservatively where known from current board use. If
  uncertain, prefer a short reserved reason over pretending the pin is generally safe.
- Export a function like `esp32c6_hardware_manifest() -> HardwareManifest`.

Add `lp-fw/fw-esp32/src/board/esp32c6/gpio_output_table.rs` if dynamic pin dispatch is practical:

- Own the concrete HAL GPIO output peripherals available for output.
- Provide a small dispatch method from numeric GPIO to `PeripheralOutput<'static>`.
- Start with GPIO18 if broad dispatch becomes too much for this milestone, but ensure other pins
  fail with a clear `OutputError::Hardware` or `InvalidConfig` and no leaked claim.

Update `lp-fw/fw-esp32/src/output/provider.rs`:

- Add a `HardwareRegistry` field initialized from the ESP32-C6 manifest, either in `new()` or a
  dedicated constructor.
- Store `HardwareLease` in `ChannelState`.
- On open, build the GPIO + RMT claim and acquire it before allocating `DisplayPipeline`.
- If later provider setup fails, release the lease before returning the error.
- Keep existing display-pipeline resize and transaction behavior intact.
- Keep `LedChannel`/`CURRENT_TRANSACTION` behavior driver-shaped; do not move RMT transmission into
  the registry.

Update `lp-fw/fw-esp32/src/main.rs` and `lp-fw/fw-esp32/src/board/esp32c6/init.rs` only as much as
needed to pass the manifest/registry into the provider. Avoid broad boot tuple churn unless required
for concrete GPIO ownership.

Risk to call out in the implementation summary: `Esp32OutputProvider::init_rmt` currently consumes
RMT and GPIO18 at boot. If true runtime pin selection needs a larger HAL ownership refactor, keep
M1 focused on resource ownership and clear errors, then record dynamic pin dispatch as a follow-up.

## Validate

```bash
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server,test_rmt
```
