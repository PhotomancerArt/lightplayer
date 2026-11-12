# Phase 3: Example And ESP32 D9

## Scope Of Phase

Add the checked-in `examples/button` project and make the ESP32 normal firmware path capable of
opening the D9/GPIO20 button endpoint.

In scope:

- `examples/button` project files.
- Shader that draws a circle while `held` is present.
- Example loader/render tests.
- ESP32 D9/GPIO20 owned-pin support for button service registration.
- `fw-emu` / host wiring for virtual button service where needed.

Out of scope:

- Generic arbitrary GPIO dispatch for all button pins.
- Radio bridge.
- Playlist/special-effect switching.

## Code Organization Reminders

- Keep example files small and readable.
- Avoid landing-page or UI work; this is a project example, not a frontend.
- Keep ESP32 HAL-specific button code under `lp-fw/fw-esp32/src/hardware/`.
- Preserve existing `test_button` GPIO4 diagnostic unless explicitly moving it is required.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

1. Add `examples/button/project.toml`.

   Include nodes:

   - `output`
   - `clock`
   - `button`
   - `shader`
   - `fixture`

2. Add `examples/button/button.toml`.

   ```toml
   kind = "Button"
   endpoint = "button:gpio:D9"
   id = 1

   [bindings.held]
   target = "bus#trigger"
   ```

3. Add `examples/button/shader.toml`.

   - Bind `output` to `bus#visual.out`.
   - Bind `events` from `bus#trigger`.
   - Consume `events` as a one-slot sentinel map of `lp::control::Message`.
   - Use a small texture size through the texture/fixture chain as existing examples do.

4. Add `examples/button/shader.glsl`.

   Render:

   - dark or low-intensity idle background;
   - a visible circle when `events[0].id != 0u`;
   - no dependency on advanced GLSL features beyond what current examples already validate.

5. Add `clock.toml`, `fixture.toml`, and `output.toml`.

   Start by copying the shape of `examples/basic` or `examples/events`, updated for endpoint-based
   output. Keep output endpoint `ws281x:rmt:D10` unless there is a specific reason to move it.

6. Add example validation.

   Existing `lp-cli/tests/examples_valid.rs` recursively loads all checked-in examples; make sure
   `examples/button` passes.

   Add an engine test similar to `events_example_merges_bus_maps_into_visual_shader`, but with
   virtual D9 button injection:

   - load `examples/button`;
   - tick unpressed and render: assert circle is absent/dim;
   - set virtual D9 pressed, tick past debounce, render: assert circle is bright;
   - release, tick past debounce, render: assert circle is absent/dim again.

7. ESP32 D9 support.

   Update `lp-fw/fw-esp32/src/board/esp32c6/init.rs`:

   - return owned `GPIO20` from `init_board()`;
   - update all call sites destructuring `init_board()`.

   Update `lp-fw/fw-esp32/src/hardware/button.rs`:

   - add an ESP32 button driver or constructor for D9/GPIO20;
   - configure `InputConfig::default().with_pull(Pull::Up)`;
   - poll as active-low with `is_low()`;
   - claim `/gpio/20` through the shared registry.

   Update normal firmware `main.rs`:

   - register the D9 button driver on the root `HardwareSystem`;
   - pass the button-capable service into loaded projects through server/engine services.

   If the existing test-only button module is behind `#[cfg(feature = "test_button")]`, move or
   split reusable button driver code so normal firmware can compile it without enabling the test
   feature.

## Validate

Run:

```bash
cargo fmt --check
cargo test -p lp-cli --test examples_valid
cargo test -p lpc-engine button_example
cargo check -p lpa-server
cargo test -p lpa-server --no-run
cargo check -p fw-emu --target riscv32imac-unknown-none-elf --profile release-emu
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server
```

