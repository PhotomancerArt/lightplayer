# Milestone 3: Compute Shader Node

## Title And Goal

Add a runtime node that resolves shader inputs, executes serial compute, and
publishes typed produced slots.

## Suggested Plan Location

`docs/roadmaps/2026-05-12-serial-compute-shaders-fluid/m3-compute-shader-node/`

## Scope

In scope:

- Add `ComputeShaderDef` / `kind = "shader/compute"` or equivalent.
- Add `ComputeShaderNode`.
- Register dynamic produced slot shapes from the artifact.
- Resolve consumed slots through the existing resolver.
- Publish produced slot values into runtime state.
- Add a small non-fluid example/test shader.

Out of scope:

- Fluid node.
- Debug UI polish.
- Persistent mutation workflows for compute slot definitions.

## Key Decisions

- Compute outputs are produced slot values, not products, for this milestone.
- The node runs once per tick/frame.
- Dynamic slot shapes come from the authored artifact.

## Deliverables

- Runtime compute shader node.
- Loader support for compute shader artifacts.
- Tests for input binding, execution, output publication, and shape sync.
- Example compute shader producing a simple typed value.

## Dependencies

- Milestone 1 shader slot model.
- Milestone 2 serial compute ABI.

## Execution Strategy

Full plan. The node must integrate model, engine, loader, resolver, and runtime
state behavior.

