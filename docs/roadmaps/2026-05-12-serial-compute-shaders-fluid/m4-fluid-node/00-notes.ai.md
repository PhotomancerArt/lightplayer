# M4 Fluid Node Notes

## Scope

- Add the first real `FluidNode` to the runtime engine.
- Move/adapt the RGB Q32 MSAFluid solver from firmware test code into engine-owned runtime code.
- Add authored and runtime model types for fluid:
  - `FluidDef`
  - `FluidState`
  - `NodeDef::Fluid`
  - `NodeKind::Fluid`
- Consume emitter data as a map-shaped slot, not as a wrapper object:
  - logical shape: `SlotData::Map<u32, FluidEmitter>`
  - authored receive slot convention: `emitters`
  - merge policy: `SlotMerge::ByKey`
- Produce a `VisualProduct` from fluid state so existing fixtures can consume it.
- Keep simulation advancement in `tick`; render/sample operations only read the current solver state.
- Add focused tests for loading, emitter consumption, simulation tick, visual-product production, and visual sampling.

## Explicitly Out Of Scope

- Wgpu/GPU fluid.
- Touch/audio input nodes.
- Full UI for fluid editing.
- End-to-end compute-fluid example polish. That belongs in the next milestone, though M4 should make it easy.
- Solving general slot merge metadata on all defs. M4 can use a fluid-specific policy hook if needed.

## Current Codebase State

- `SlotFieldShape` currently stores only:
  - `name`
  - `shape`
- `SlotMeta` currently lives on container shapes, not record fields, and is presentation-oriented.
- `SlotRecord` derive currently emits field shapes through:
  - `::lpc_model::slot::shape::field(name, shape)`
- The derive parser already handles field attributes in:
  - `lp-core/lpc-slot-macros/src/attr.rs`
  - `lp-core/lpc-slot-macros/src/record.rs`
- Adding field semantics is therefore a focused model + macro extension, not a full generator rewrite.
- `FluidEmitter` already exists in `lpc-model` at:
  - `lp-core/lpc-model/src/nodes/fluid/fluid_emitter.rs`
  - native shape name: `lp::fluid::Emitter`
  - complete `SlotValue` leaf with `ToLpValue` / `FromLpValue`
  - docs already say emitter collections should be `MapSlot<u32, FluidEmitter>` or equivalent map-shaped slot data.
- There is no `FluidDef`, `FluidState`, `FluidNode`, or `NodeDef::Fluid` yet.
- `NodeDef` currently lives in:
  - `lp-core/lpc-model/src/nodes/node_def.rs`
  - variants: `Project`, `Texture`, `Shader`, `ComputeShader`, `Output`, `Fixture`.
- `NodeKind` currently lives in:
  - `lp-core/lpc-model/src/node/kind.rs`
  - variants mirror the current `NodeDef` set.
- Runtime visual production pattern is established:
  - `ShaderNode` stores `ShaderState { output: VisualProductSlot }`.
  - `ShaderNode::tick` stamps `VisualProduct::new(node_id, 0)`.
  - `ShaderNode` implements `RenderNode`.
  - The engine reads produced slots through `runtime_state_slots`, not `ProducedSlotAccess`.
- Fixture consumption pattern is established:
  - `FixtureNode` resolves its consumed `input` slot.
  - It accepts `LpValue::Product(ProductRef::Visual(_))`.
  - Direct fixture sampling calls `RenderNode::sample_visual_into`.
- Authored def fallback is established:
  - `EngineResolveHost::produce(QueryKey::ConsumedSlot)` reads the node def from `ArtifactStore`.
  - `EngineResolveHost::produce(QueryKey::ConsumedSlotAccessor)` uses generated slot views/accessors.
- Aggregate merge support exists from M3.5:
  - `SlotMerge::{Latest, Error, ByKey}`.
  - `ResolveHost::merge_policy_for_consumed_slot`.
  - `ResolveSession` can merge multiple map-shaped binding inputs by key.
  - Bus-to-bus and many-to-one binding paths are already supported in the resolver.
- The solver prototype currently lives under firmware test code:
  - `lp-fw/fw-esp32/src/tests/msafluid_solver.rs`
  - `lp-fw/fw-esp32/src/tests/fluid_demo/emitters.rs`
  - `lp-fw/fw-esp32/src/tests/fluid_demo/sampler.rs`
  - `lp-fw/fw-esp32/src/tests/fluid_demo/runner.rs`
- The solver is already `alloc`/Q32-oriented and should adapt cleanly to `lpc-engine`.
- `lpc-engine` already depends on `lps-q32` and `libm`, so M4 should not need a dependency bump for the solver.

## User Notes And Decisions

- The old milestone text says `FluidEmitterSet`, but that is stale. We should not add a wrapper unless a concrete need appears.
- Natural emitter data is a map:
  - stable key identity
  - per-emitter replacement/removal
  - merge-by-key across multiple producers
- Compute shaders do not know about maps internally. Mapping shader array output into map-shaped slot data is owned by shader slot mapping code, already started in M3.
- Receiver-owned merge policy is the right model:
  - produced slots do not declare how they merge
  - consumed slots decide how multiple bindings are handled
  - fluid `emitters` should use `merge = "by_key"` eventually
- For M4, a special-case fluid merge hook is acceptable if it keeps the slice small.
- Fluid should advance in `tick`; visual sampling/rendering should never advance the simulation.
- Pixel/sample hot paths should avoid `f32` where reasonably possible on ESP32. It is still acceptable for authored defs and low-frequency node config.
- First defaults should be ESP32-realistic.

## Solver Context From Prototype

- Current demo defaults from `fluid_demo/runner.rs`:
  - grid: `20x20`
  - solver iterations: `3`
  - solver target: `25 Hz`
  - fade speed: `0.1`
  - viscosity: `0.00003`
- Current solver API:
  - `MsaFluidSolver::new(nx, ny)`
  - `set_solver_iterations`
  - `set_fade_speed`
  - `set_viscosity`
  - `add_force_at_cell`
  - `add_color_at_cell`
  - `update`
  - read channels with `r(x,y)`, `g(x,y)`, `b(x,y)`
- Current emitter prototype is hardcoded/demo-oriented. M4 should extract only the useful stamping ideas, not the pulser.
- Current sampler prototype supports nearest and bilinear sampling, but uses some `f32` boundary math. M4 can start simple and tighten fixed-point math where it matters.

## Suggested Fluid Def Shape

Initial suggested authored shape:

```rust
pub struct FluidDef {
    pub bindings: BindingDefs,
    pub size: Dim2uSlot,
    pub solver_iterations: ValueSlot<u32>,
    pub fade_speed: RatioSlot,
    pub viscosity: PositiveF32Slot,
}
```

Possible additions, depending on desired first slice:

- `step_hz` / `tick_rate`: useful if fluid should advance slower than engine frames.
- `sampling`: nearest/bilinear option. Could default to bilinear and wait.
- `input_gain`: probably unnecessary because each `FluidEmitter` already has `intensity`.

## Open Questions

### Q1. Do we add `step_hz` in M4?

Context:

- The prototype advanced the solver at about 25 Hz while display/render could run faster.
- The engine currently ticks nodes every frame when demanded.
- Adding rate limiting now means `FluidNode::tick` may skip solver update while still publishing the same visual product.

Suggested answer:

- Include `step_hz` as a low-frequency config slot with default `25`.
- Keep it simple: accumulate elapsed time from `ctx.time_seconds()` and advance at most once per tick for now. Do not build catch-up loops unless tests show they are needed.

### Q2. Where should `SlotMerge::ByKey` for `fluid.emitters` be declared?

Context:

- The receiver owns merge behavior.
- `FluidDef` is the authored/default slot root for a fluid node.
- A node def slot can have default authored data and still be a consumed slot.
- The resolver already asks `ResolveHost::merge_policy_for_consumed_slot(node, slot)`.
- `SlotMeta` is currently human/tool presentation metadata; merge policy changes resolver behavior.

Decision:

- Add first-class slot semantics carried by slot field shape, separate from presentation `SlotMeta`.
- Preferred name: `SlotSemantics`.
- `SlotSemantics` should cover graph/dataflow behavior such as:
  - direction: local/default, consumed, produced
  - merge policy
  - required/optional, if needed
- Rust authoring should be terse:

```rust
#[slot(consumed, merge = "by_key")]
pub emitters: MapSlot<u32, FluidEmitter>,
```

- The generated/root shape should carry that semantic metadata.
- The resolver should derive `merge_policy_for_consumed_slot` from the node def shape/semantics instead of hard-coding `FluidDef`.
- `emitters` remains real slot data on `FluidDef`, usually empty by default, so unbound fluid nodes can still have authored/static emitters for tests and simple projects.

Suggested concrete shape:

```rust
pub struct SlotFieldShape {
    pub name: SlotName,
    pub shape: SlotShape,
    #[serde(default)]
    pub semantics: SlotSemantics,
}

pub struct SlotSemantics {
    pub direction: SlotDirection,
    pub merge: SlotMerge,
}

pub enum SlotDirection {
    Local,
    Consumed,
    Produced,
}
```

Initial defaults:

- direction: `Local`
- merge: `Latest`

Macro surface:

```rust
#[slot(consumed, merge = "by_key")]
pub emitters: MapSlot<u32, FluidEmitter>,
```

Implementation notes:

- Existing fields keep `SlotSemantics::default()`.
- `shape::field(name, shape)` can keep returning a local/latest field.
- Add `shape::field_with_semantics(name, shape, semantics)` or equivalent.
- Keep `required` out of M4 unless it becomes necessary; binding validation can start with direction.
- For produced runtime state such as `ShaderState.output` and `FluidState.output`, use `#[slot(produced)]` once the macro supports direction.

### Q3. Should M4 update `examples/basic`, or wait for M5?

Context:

- The roadmap’s M5 is the end-to-end fluid compute example.
- M4 can be proven with unit tests and maybe a small fixture/helper project.
- Updating `examples/basic` too early may obscure whether performance changes came from fluid or fixture/shader work.

Suggested answer:

- Do not change `examples/basic` in M4.
- Add targeted tests and leave the visible compute-fluid example for M5.

### Q4. What first sampling quality should `FluidNode` expose?

Context:

- Fixture direct sampling now calls `sample_visual_into`.
- Fluid naturally maps normalized `VisualSamplePoint` coordinates to solver grid.
- Nearest is cheaper; bilinear looks better and the prototype already has it.

Suggested answer:

- Implement nearest first if it keeps the phase smaller, then add bilinear in the same plan if straightforward.
- Expose no authored sampling option in M4 unless both modes are implemented.

### Q5. How should emitter stamping interpret `FluidEmitter::dir`?

Context:

- `FluidEmitter` contains `pos`, `dir`, `radius`, `color`, `velocity`, `intensity`.
- The prototype has helpers for directional force/color injection.

Suggested answer:

- Treat `pos` as normalized texture/fluid coordinates.
- Treat `dir` as a direction vector.
- Stamp color in a radius around `pos` using `color * intensity`.
- Stamp force using normalized `dir * velocity * intensity`.
- Clamp to grid bounds and tolerate invalid/zero-length directions.

## Likely Plan Shape

- Phase 1: model types and node loader shape.
- Phase 2: move/adapt solver and sampler.
- Phase 3: implement `FluidNode` tick/merge consumption/render-node sampling.
- Phase 4: tests for model, resolver integration, node runtime, and sampling.
- Phase 5: cleanup/docs/validation.
