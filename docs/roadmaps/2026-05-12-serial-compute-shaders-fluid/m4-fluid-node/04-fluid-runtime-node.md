# Phase 4: Fluid Runtime Node

## Scope Of Phase

Implement `FluidNode`, connect it to the engine loader, and make it produce a visual product.

In scope:

- Add `FluidNode`.
- Register fluid runtime state shape.
- Use `FluidDefView` for config and consumed emitter access.
- Resolve `emitters` through the resolver.
- Produce `FluidState.output`.
- Implement `RenderNode` sampling.
- Register fluid bindings in the project loader.
- Derive merge policy from `SlotSemantics`.

Out of scope:

- Full `examples/basic` update.
- Polished UI rendering.
- Multiple solver backends.

## Code Organization Reminders

- Keep `fluid_node.rs` focused on node lifecycle.
- Keep solver math in `solver.rs`.
- Keep stamping and sampling helpers out of `fluid_node.rs`.
- Tests go at the bottom of relevant files or in existing engine test modules.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests.
- If blocked, stop and report.
- Report changed files, validation, and deviations.

## Implementation Details

Relevant files:

- `lp-core/lpc-engine/src/nodes/mod.rs`
- `lp-core/lpc-engine/src/nodes/fluid/mod.rs`
- `lp-core/lpc-engine/src/nodes/fluid/fluid_node.rs`
- `lp-core/lpc-engine/src/engine/project_loader.rs`
- `lp-core/lpc-engine/src/engine/engine.rs`
- `lp-core/lpc-engine/src/node/node_runtime.rs`
- `lp-core/lpc-engine/src/node/render_node.rs`

`FluidNode` should own:

- `FluidState`
- `Option<FluidDefView>`
- optional solver
- cached solver config
- last solver step time

Tick flow:

1. Compile/get `FluidDefView`.
2. Resolve config slots.
3. Ensure solver exists and matches `size`.
4. Resolve `emitters`.
5. Stamp emitter map into solver.
6. Step solver according to `step_hz`, at most once per tick for M4.
7. Set `FluidState.output` to `VisualProduct::new(ctx.node_id(), 0)`.

Rendering:

- `render_node()` returns `Some(self)`.
- `sample_visual_into` samples current solver state.
- `render_texture_into` can be implemented by iterating texture pixels and sampling; if this is too large, leave `render_texture` unsupported and rely on direct fixture sampling tests.

Loader:

- Add `NodeDef::Fluid` branch.
- Attach `FluidNode::new(node.id, frame)` or similar.
- Register optional source binding for `emitters`.
- Register target binding for `output`.

Merge policy:

- Implement generic lookup in `EngineResolveHost::merge_policy_for_consumed_slot`.
- It should read the node def shape field semantics for the consumed slot.
- If the slot is missing, default to `Latest`.

Tests:

- Fluid node with authored inline emitters produces a visual product.
- Fluid node with bound emitter map consumes the bound aggregate.
- Two providers through a bus merge emitter maps by key.
- Sampling the fluid output after a tick returns nonzero color near the emitter.

## Validate

```bash
cargo fmt --check
cargo test -p lpc-engine fluid
cargo test -p lpc-engine resolver
cargo check -p lpc-engine
```

