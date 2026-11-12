# M4 Summary

## Completed

- Added slot direction and merge semantics to slot field shapes.
- Extended `#[derive(SlotRecord)]` with `#[slot(consumed)]`, `#[slot(produced)]`, and `#[slot(merge = "...")]`.
- Upgraded static shape codegen so manual `impl StaticSlotShape for Type` native value roots are registered with `#[slot(root)]` records.
- Added `FluidDef`, `FluidState`, and `NodeDef::Fluid`.
- Added the `FluidNode` runtime node.
- Ported the MSA-style Q32 fluid solver into `lpc-engine`.
- Added emitter stamping and nearest-neighbor visual sampling helpers.
- Wired `kind = "fluid"` into the project loader.
- Derived consumed-slot merge policy from the authored node def shape.
- Proved compute-shader emitter maps can flow through a bus into a fluid node.

## Validation

```bash
cargo fmt --check
cargo test -p lpc-slot-codegen
cargo test -p lpc-model
cargo test -p lpc-engine
cargo check -p lpc-engine
```

All passed.

## Follow-Ups

- Add the end-to-end example in M5: compute shader -> fluid -> fixture/output.
- Decide how aggressively to expose slot semantics in the debug UI.
- Add more efficient fixed-point fluid sampling if nearest-neighbor artifacts are too rough.
- Generalize consumed aggregate validation so non-map inputs fail earlier and with better diagnostics.
- Revisit native value-root registration if we introduce shared shapes outside `lpc-model`.
