# Separate Engine Runtime State Shapes

## Status

Implemented in the static-shapes followup pass after rebasing onto
`feature/radio`.

The engine now uses `RuntimeStateShape` for fixed node-owned runtime state
roots, while authored model/support shapes resolve through `SlotShapeLookup`
instead of being inserted into the dynamic registry as a side effect of runtime
state registration.

## Smell

Engine node shape registration still mixes several concepts:

- authored model shapes that now belong in the generated static catalog
- runtime state shapes that are fixed Rust-authored shapes
- dynamic artifact or instance shapes

This is much better than the old global bootstrap, but the boundary is still
not obvious from call sites like `ensure_registered`.

## Better Shape

Make engine runtime-state shape registration its own named concept. Authored
model defs should rely on catalog lookup; dynamic artifact shapes should remain
dynamic registry entries; runtime-state shapes should have a separate path.

This should make accidental reintroduction of authored static bootstrap easier
to spot in review.

## Useful Context

- `NodeRuntime::register_runtime_state_shapes`
- node implementations under `lp-core/lpc-engine/src/nodes`
- `SlotShapeLookup` fallback behavior in `lpc-model`
