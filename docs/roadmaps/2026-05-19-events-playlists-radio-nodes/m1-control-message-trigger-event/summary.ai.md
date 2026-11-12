# M1 Implementation Summary

Implemented M1 as the smallest usable control-event path:

- Added `ControlMessage { id: u32, seq: u32 }` in `lpc-model`, with `TriggerEvent` as a semantic
  alias for no-payload trigger events.
- Kept `address`, `args`, `source`, timetags, and `u64` IDs deferred.
- Added shared shader input materialization so both compute and visual shaders can consume
  sentinel maps as fixed shader ABI arrays.
- Allowed equal-priority bus providers to coexist so map consumers can merge by key, while direct
  bus resolution remains ambiguous when there is no merge-aware consumer.
- Added compute shader descriptor/header support for consumed sentinel maps.
- Added LPVM struct argument flattening so array-of-struct shader inputs can be passed through the
  existing call path.
- Added `examples/events`, where two compute shaders produce `ControlMessage` maps into
  `bus#trigger` and a visual shader consumes the merged map to draw active event circles.
- Updated checked-in examples and the CLI project template to use authored hardware endpoint specs
  instead of the removed legacy `pin` field.

Validation run:

```bash
cargo fmt --check
cargo test -p lpc-model control_message
cargo test -p lpc-engine shader_input_materialize
cargo test -p lpc-engine compute_desc_accepts_consumed_sentinel_maps
cargo test -p lpc-engine events_example_merges_bus_maps_into_visual_shader
cargo test -p lp-cli --test examples_valid
cargo test -p lpc-engine --test runtime_spine
cargo test -p lpc-engine engine_services
cargo check -p lpc-model --no-default-features
cargo check -p lpa-server
cargo test -p lpa-server --no-run
cargo check -p fw-emu --target riscv32imac-unknown-none-elf --profile release-emu
```
