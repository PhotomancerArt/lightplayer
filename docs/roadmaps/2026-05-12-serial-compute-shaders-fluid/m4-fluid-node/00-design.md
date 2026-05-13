# M4 Fluid Node Design

## Scope

M4 adds the first real fluid runtime node and the slot-semantics model it needs:

- `SlotSemantics` on record fields.
- `FluidDef` as an authored/default slot root.
- `FluidState` as a produced runtime state root.
- `FluidNode` as a stateful visual producer.
- Engine loading/resolution support for semantic merge policy.
- Focused tests for slot semantics, fluid loading, emitter consumption, solver stepping, and sampling.

M4 does not build the polished end-to-end compute-fluid example. That remains M5.

## File Structure

```text
lp-core/lpc-model/src/slot/
  slot_direction.rs
  slot_semantics.rs
  slot_shape.rs
  slot_shape_builder.rs

lp-core/lpc-slot-macros/src/
  attr.rs
  record.rs

lp-core/lpc-model/src/nodes/fluid/
  mod.rs
  fluid_def.rs
  fluid_emitter.rs
  fluid_state.rs

lp-core/lpc-engine/src/nodes/fluid/
  mod.rs
  fluid_node.rs
  solver.rs
  sampler.rs
  emit.rs

lp-core/lpc-engine/src/engine/
  project_loader.rs
  engine.rs
```

## Architecture Summary

`NodeDef` is the authored/default slot root for a node. A field on that root can be:

- local config: authored data only, resolved as fallback defaults
- consumed: authored default data plus optional bindings
- produced: generally used on runtime state roots

Field behavior is stored in `SlotSemantics`, separate from presentation-oriented `SlotMeta`.

```rust
pub enum SlotDirection {
    Local,
    Consumed,
    Produced,
}

pub struct SlotSemantics {
    pub direction: SlotDirection,
    pub merge: SlotMerge,
}
```

`SlotFieldShape` carries these semantics:

```rust
pub struct SlotFieldShape {
    pub name: SlotName,
    pub shape: SlotShape,
    pub semantics: SlotSemantics,
}
```

The slot macro supports semantic field annotations:

```rust
#[slot(consumed, merge = "by_key")]
pub emitters: MapSlot<u32, FluidEmitter>,
```

Unannotated fields default to `direction = Local` and `merge = Latest`.

## Fluid Def

`FluidDef` is a normal root slot record:

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, lpc_slot_macros::SlotRecord)]
#[slot(root, view)]
pub struct FluidDef {
    #[serde(default, skip_serializing_if = "BindingDefs::is_empty")]
    pub bindings: BindingDefs,

    pub size: Dim2uSlot,
    pub solver_iterations: ValueSlot<u32>,
    pub step_hz: PositiveF32Slot,
    pub fade_speed: RatioSlot,
    pub viscosity: PositiveF32Slot,

    #[slot(consumed, merge = "by_key")]
    #[slot(map(key = "u32", value_ref = "lp::fluid::Emitter"))]
    #[serde(default, skip_serializing_if = "MapSlot::is_empty")]
    pub emitters: MapSlot<u32, FluidEmitter>,
}
```

This means `emitters` can be authored directly in TOML for tests/simple projects, but production projects usually bind it:

```toml
[bindings.emitters]
source = "bus#fluid.emitters"
```

## Resolver Semantics

`EngineResolveHost::merge_policy_for_consumed_slot` should derive merge policy from the node def shape:

1. Resolve the node entry.
2. Resolve the node’s authored def through its `NodeDefHandle`.
3. Look up the consumed slot’s field shape.
4. Return `field.semantics.merge`.
5. Default to `Latest` if the slot is missing or has default semantics.

This avoids a fluid-specific resolver special case and establishes the receiver-owned merge-policy model.

## Fluid Runtime

`FluidNode` owns:

- `FluidState { output: VisualProductSlot }`
- cached `FluidDefView`
- an optional solver instance
- cached solver config
- last solver step time

`tick`:

1. Resolve config through `FluidDefView`.
2. Ensure solver exists and matches `size`.
3. Resolve `emitters` through the resolver/view.
4. Stamp emitters into the solver.
5. Advance the solver according to `step_hz`, at most once per tick for M4.
6. Publish `VisualProduct::new(node_id, 0)`.

`RenderNode`:

- `sample_visual_into` samples current solver state into `rgba_unorm16`.
- `render_texture_into` may fill a texture from current solver state if straightforward.
- Rendering and sampling never advance simulation.

## Validation

Final validation for the plan:

```bash
cargo fmt --check
cargo test -p lpc-model
cargo test -p lpc-engine
cargo check -p lpc-engine
```

Run narrower commands during phases when useful.

