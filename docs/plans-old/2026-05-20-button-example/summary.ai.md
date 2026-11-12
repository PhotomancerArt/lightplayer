# Button Example Implementation Summary

Implemented the first hardware input slice for `examples/button`.

## Done

- Added `kind = "Button"` to the authored model with `endpoint`, `id`, and `stable_ms` slots.
- Added runtime `ButtonState` maps named `down`, `held`, and `up`, each carrying `lp::control::Message` values.
- Added `ButtonNode`, which opens a debounced hardware button input and publishes transition/held maps.
- Added `ButtonService` plumbing through `EngineServices`, `TickContext`, `LpServer`, `Project`, and `ProjectManager`.
- Added shared `HardwareSystem::open_button_by_spec` and made `VirtualButtonDriver` cloneable for tests/control handles.
- Wired `fw-emu` to expose its virtual hardware system as the button service.
- Wired `fw-esp32` to expose D9/GPIO20 through an internal-pull-up button driver.
- Updated ESP32 board init/test destructuring for GPIO20.
- Fixed the XIAO ESP32-C6 board profile display label for GPIO20 from `d9` to `D9`.
- Added `examples/button`, where `button:gpio:D9` feeds `bus#trigger` through the `held` map and turns on a shader circle while pressed.

## Validation

- `cargo check -p lpc-engine`
- `cargo check -p lpa-server`
- `cargo check -p fw-emu --target riscv32imac-unknown-none-elf --profile release-emu`
- `cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server`
- `cargo test -p lpc-model button_def`
- `cargo test -p lpc-shared virtual_system_opens_button_by_endpoint_spec`
- `cargo test -p lpc-engine button_node_publishes_held_and_up_from_virtual_d9`
- `cargo test -p lp-cli checked_in_examples_load_as_core_projects`
