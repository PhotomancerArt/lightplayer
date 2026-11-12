# Milestone 1: Shader Slot Shape Model

## Title And Goal

Define a general authored shader slot model that can represent compute shader
inputs and outputs.

## Suggested Plan Location

`docs/roadmaps/2026-05-12-serial-compute-shaders-fluid/m1-shader-slot-shape-model/`

## Scope

In scope:

- Generalize existing shader param ideas into shader slot definitions.
- Represent consumed and produced shader slots in TOML.
- Support enough value shapes for fluid emitters:
  - scalar;
  - vectors;
  - structs;
  - fixed/bounded arrays.
- Add semantic `FluidEmitter` / `FluidEmitterSet` value shapes in `lpc-model`.
- Generate or sketch shader header text from TOML definitions.

Out of scope:

- Running compute shaders.
- Fluid node runtime.
- Full GLSL source annotation parsing.

## Key Decisions

- TOML is source of truth for shape.
- Header generation is an ergonomics layer.
- Fluid emitter values should be bounded for runtime.

## Deliverables

- Model types for compute shader slot definitions.
- `FluidEmitterSet` slot value shape.
- TOML round-trip tests for a compute shader artifact.
- Header generation evidence for the first compute emitter shader.

## Dependencies

- Current slot/value shape machinery.
- Existing shader param-def mockup lessons.

## Execution Strategy

Full plan. This milestone establishes domain vocabulary and shape semantics
that later milestones depend on.

