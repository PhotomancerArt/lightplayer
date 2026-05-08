# M2.4 Runtime Node Truth Pass Summary

## Completed

- Removed the old node-specific projection hooks from `NodeRuntime`.
- Added lazy full-texture materialization to the render product API.
- Added `ShaderRenderProduct`, which compiles and renders on demand through `RenderProductStore`.
- Simplified `ShaderNode` so it owns and publishes an `output` render-product slot instead of eagerly rendering into a texture-backed product on tick.
- Reworked `FixtureNode` to consume its `input` slot, materialize the render product into a full texture, and write fixture output buffers.
- Added fixture-owned `render_size` to `FixtureDef`.
- Updated project loading so shader output bindings and fixture input bindings are registered from authored `BindingDefs`.
- Flattened `examples/basic` to the MVP flow: shader produces to `bus#visual.out`, fixture consumes from that bus, output remains the sink.

## Validation

- `cargo check -p lpc-engine`
- `cargo test -p lpc-engine`
- `cargo test -p lpc-model`
- `cargo check -p lpc-shared`
- `cargo check -p lpc-source`
- `cargo test -p lpc-source --test basic_example_parse`
- `cargo test -p lpc-slot-mockup`
- `cargo clippy -p lpc-engine -p lpc-model -p lpc-source -p lpc-shared -p lpc-slot-mockup --all-targets -- -D warnings`
