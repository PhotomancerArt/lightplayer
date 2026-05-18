# Phase 4: ESP32 Button Diagnostic

## Scope Of Phase

In scope:

- Add ESP32 GPIO button input using internal pull-up and button-to-ground
  wiring.
- Claim the button GPIO through the root-owned hardware registry.
- Add a firmware diagnostic/test mode that logs debounced button events.
- Use GPIO4 as the first concrete button pin unless implementation context
  clearly indicates a better already-owned pin.

Out of scope:

- Dynamic arbitrary GPIO input dispatch.
- Project graph integration.
- ESP-NOW radio send/receive.
- Production UI for selecting a button pin.

## Code Organization Reminders

- Keep ESP32 HAL-specific button code under `lp-fw/fw-esp32/src/hardware/` or
  another firmware hardware module, not in shared model crates.
- Keep the debouncer shared and HAL-independent.
- Mark any hardcoded GPIO4 diagnostic choice clearly as temporary.
- Tests stay at the bottom of files.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

1. Add an ESP32 button type.

   Suggested API:

   ```rust
   pub struct Esp32ButtonInput { ... }

   impl Esp32ButtonInput {
       pub fn open_gpio4(
           registry: Rc<HardwareRegistry>,
           pin: esp_hal::peripherals::GPIO4<'static>,
           config: ButtonConfig,
       ) -> Result<Self, HardwareError>;

       pub fn poll(&mut self, now_ms: u64) -> Option<ButtonEvent>;
   }
   ```

   Use whatever ESP HAL input type and pull-up configuration is idiomatic for
   the current `esp-hal` version.

2. Claim `/gpio/4` before configuring or polling the button.

   Steps:

   - Ensure `/gpio/4` has `HardwareCapability::GpioInput`.
   - Claim it with claimant name such as `"esp32-button"`.
   - Store the `HardwareLease`.
   - Release the lease on explicit close or drop.

3. Configure internal pull-up and active-low logic.

   The physical wiring is normally-open button to GND:

   - raw high means released
   - raw low means pressed

4. Add feature-gated diagnostic firmware mode.

   Suggested feature name: `test_button` or `test_gpio_button`.

   Diagnostic behavior:

   - Boot board and runtime.
   - Mount or create the filesystem if needed to load `/hardware.toml`.
   - Create root hardware registry.
   - Open GPIO4 button.
   - Print/log debounced events as lines such as:

     ```text
     BUTTON gpio=/gpio/4 seq=1 kind=pressed
     BUTTON gpio=/gpio/4 seq=2 kind=released
     ```

5. Update `lp-fw/fw-esp32/Cargo.toml`, `main.rs`, and module gates for the new
   test mode.

6. Preserve existing `test_gpio_calibrate` behavior.

   Do not reuse calibration's `AnyPin::steal` approach in production button
   code unless the implementation is explicitly diagnostic-only and documented
   as such.

## Validate

```bash
cargo fmt --check
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,test_button
```

If the feature is named differently, use the actual feature name in the second
command and update this phase file during execution.

