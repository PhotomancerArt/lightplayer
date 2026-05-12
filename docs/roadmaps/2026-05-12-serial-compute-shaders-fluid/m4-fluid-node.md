# Milestone 4: Fluid Node

## Title And Goal

Promote the fluid solver into an engine node that consumes emitter data and
produces a visual product.

## Suggested Plan Location

`docs/roadmaps/2026-05-12-serial-compute-shaders-fluid/m4-fluid-node/`

## Scope

In scope:

- Move/adapt the RGB Q32 MSAFluid solver from firmware test code.
- Add `FluidDef` and `FluidState`.
- Add `FluidNode` as a stateful visual producer.
- Consume `FluidEmitterSet`.
- Implement `RenderNode` for direct sampling and optional texture rendering.
- Keep tick as the only simulation-advancing operation.

Out of scope:

- Wgpu fluid implementation.
- Touch/audio input nodes.
- Advanced fluid UI.

## Key Decisions

- Fluid advances in `tick`.
- Render/sample reads current state only.
- First defaults should be ESP32-realistic.

## Deliverables

- Fluid node model and runtime.
- Unit tests for emitter consumption, simulation tick, and visual product
  sampling.
- Basic memory-pressure handling for solver buffers.

## Dependencies

- `FluidEmitterSet` from Milestone 1.
- Current visual product/render node architecture.
- Preferably Milestone 3 compute node for end-to-end use, though local tests can
  feed emitters directly.

## Execution Strategy

Full plan. The solver exists, but integrating it cleanly into the node/product
runtime needs careful ownership, memory, and tick semantics.

