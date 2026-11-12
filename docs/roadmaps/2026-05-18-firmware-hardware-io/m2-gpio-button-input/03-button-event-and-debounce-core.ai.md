# Phase 3: Button Event And Debounce Core

## Scope Of Phase

In scope:

- Add shared button event vocabulary.
- Add deterministic debounce logic.
- Add a small virtual/mock button claimant for host and emu tests.
- Prove button claims use the same hardware registry vocabulary as outputs.

Out of scope:

- ESP32 HAL GPIO setup.
- Firmware diagnostic loop.
- Radio message transport.
- Project graph event semantics.

## Code Organization Reminders

- Use search-friendly filenames such as `button_event.rs`,
  `button_debouncer.rs`, and `virtual_button.rs`.
- Keep public types near the top, helper functions lower, tests at the bottom.
- Keep event payloads small and no_std friendly.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

1. Add shared button modules under `lp-core/lpc-shared/src/hardware/`.

   Suggested types:

   ```rust
   pub enum ButtonEventKind {
       Pressed,
       Released,
   }

   pub struct ButtonEvent {
       pub source: HardwareAddress,
       pub sequence: u32,
       pub kind: ButtonEventKind,
   }
   ```

   If storing `HardwareAddress` in every event feels too allocation-heavy,
   use source address in the button object and keep the emitted event as
   `{ sequence, kind }`. Record that choice in comments/tests.

2. Add `ButtonDebouncer`.

   Requirements:

   - Configurable stable interval, default suitable for a large physical button
     such as 20-50 ms.
   - Active-low interpretation can live outside the debouncer; the debouncer
     should operate on logical pressed/not pressed samples.
   - It emits `Pressed` and `Released` only when the logical state has been
     stable for the configured interval.
   - It increments sequence once per emitted event.
   - It does not require `std`.

3. Add a virtual/mock button claimant.

   Suggested role:

   - Claim one `HardwareCapability::GpioInput` resource from
     `Rc<HardwareRegistry>`.
   - Release the lease on drop or through explicit close.
   - Feed logical samples into `ButtonDebouncer`.

   This can live in `lpc-shared` if it is general and no_std, or in tests if it
   is only test scaffolding.

4. Add conflict tests with `MemoryOutputProvider` using the same registry:

   - Button claims `/gpio/4`; output opening pin 4 fails.
   - Output opens pin 4; button claiming `/gpio/4` fails.
   - Output opens pin 18; button claiming `/gpio/4` succeeds.
   - Reserved `/gpio/12` cannot be claimed by a button.

## Validate

```bash
cargo fmt --check
cargo test -p lpc-shared hardware
cargo test -p lpc-shared output
cargo check -p lpc-shared --no-default-features
```

