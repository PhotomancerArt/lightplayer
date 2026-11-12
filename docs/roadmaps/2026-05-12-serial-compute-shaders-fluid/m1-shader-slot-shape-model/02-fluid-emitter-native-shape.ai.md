# Phase 02: Fluid Emitter Native Shape

## Scope Of Phase

Add `FluidEmitter` as a native LightPlayer semantic value shape.

In scope:

- Add `nodes/fluid/mod.rs`.
- Add `nodes/fluid/fluid_emitter.rs`.
- Register or expose the native shape name `lp::fluid::Emitter`.
- Ensure `SlotShapeRegistry` can resolve named roots well enough for shader slot
  refs and tests.
- Add tests for `FluidEmitter` to/from `LpValue` and shape registration.

Out of scope:

- `FluidEmitterSet`.
- Fluid node runtime.
- Solver integration.

## Code Organization Reminders

- Use explicit native shape-name constants near the native type.
- Keep serde and value conversion tests at the bottom.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not weaken no_std compatibility.

## Implementation Details

Relevant files:

- `lp-core/lpc-model/src/nodes/mod.rs`
- `lp-core/lpc-model/src/nodes/fluid/mod.rs`
- `lp-core/lpc-model/src/nodes/fluid/fluid_emitter.rs`
- `lp-core/lpc-model/src/slot/slot_shape_registry.rs`
- `lp-core/lpc-model/src/lib.rs`

`FluidEmitter` should be an opaque slot value leaf, not a slot record. Suggested
fields:

- `id: u32`
- `pos: [f32; 2]`
- `dir: [f32; 2]`
- `radius: f32`
- `color: [f32; 3]`
- `velocity: f32`
- `intensity: f32`

The semantic collection is a map slot elsewhere:

```rust
MapSlot<u32, FluidEmitter>
```

## Validate

```bash
cargo fmt --check
cargo test -p lpc-model fluid
cargo test -p lpc-model slot_shape_registry
```

