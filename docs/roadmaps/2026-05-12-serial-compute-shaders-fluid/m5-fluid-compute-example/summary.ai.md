# M5 Summary

## What Was Built

- Added `examples/fluid` as a standalone compute-fluid project.
- Wired the example as `compute -> fluid -> fixture -> output` using bus-first bindings.
- Added a serial compute shader that emits `lp::fluid::Emitter` map data.
- Added engine validation that loads the real example from disk, resolves compute emitters, resolves fluid output, renders nonzero RGBA16 visual data, and verifies compute/fluid runtime state roots are visible through project read.

## Decisions For Future Reference

#### Keep Basic Boring

- **Decision:** Leave `examples/basic` as the simple shader/fixture/output baseline and add `examples/fluid` separately.
- **Why:** Basic remains the steady-state sanity/perf fixture; fluid exercises the richer compute/slot/merge path.
- **Rejected alternatives:** Replace or complicate `examples/basic`.

#### Bus-First Fluid Flow

- **Decision:** The example binds compute emitters through `bus#fluid.emitters` and fluid visual output through `bus#visual.out`.
- **Why:** This matches the library-friendly authoring model where visuals and consumers are decoupled by conventional buses.
- **Rejected alternatives:** Direct node-to-node references in the example.

## Validation

```bash
cargo test -p lpc-model fluid -- --nocapture
cargo test -p lpc-engine fluid -- --nocapture
cargo test -p lpc-engine project_loader -- --nocapture
cargo test -p lpc-engine project_read -- --nocapture
cargo run -p lp-cli -- dev examples/fluid
cargo fmt --check
cargo test -p lpc-model
cargo test -p lpc-engine
cargo check -p lpc-engine
```

All passed. `lp-cli dev examples/fluid` built and launched cleanly in this environment.

## Follow-Ups

- Run `examples/fluid` on ESP32-C6 and capture a profile.
- Tune the emitter shader and fluid defaults after seeing real device output.
- Decide how the debug UI should visually distinguish compute-produced maps from visual products.
