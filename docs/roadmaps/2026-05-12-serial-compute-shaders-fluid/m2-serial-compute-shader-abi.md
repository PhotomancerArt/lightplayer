# Milestone 2: Serial Compute Shader ABI

## Title And Goal

Teach the shader stack to execute a serial compute shader once and copy typed
slot values in and out.

## Suggested Plan Location

`docs/roadmaps/2026-05-12-serial-compute-shaders-fluid/m2-serial-compute-shader-abi/`

## Scope

In scope:

- Add a compute shader compile/execute path alongside visual pixel shaders.
- Define a minimal serial ABI:
  - write input globals;
  - call `main`;
  - read output globals.
- Support the first fluid emitter shapes.
- Add compiler/type validation that TOML shape and shader ABI agree.
- Add host/emu tests for simple scalar/vector/struct/fixed-array outputs.

Out of scope:

- GPU workgroups or wgpu compute.
- Multiple dispatch instances.
- Arbitrary dynamic allocation inside shader code.
- Fluid node integration.

## Key Decisions

- Serial compute is a data-program ABI, not GPU compute.
- Struct and array layout must be explicit enough for native and future wgpu
  backends to share semantics.

## Deliverables

- Shader engine API for compiling/executing serial compute shaders.
- First native backend support for compute entry execution.
- Tests proving values can move from Rust -> shader -> Rust.
- Diagnostics for shape/ABI mismatch.

## Dependencies

- Milestone 1 shader slot shape model.
- Existing LPIR/native shader infrastructure.

## Execution Strategy

Full plan. This touches frontend, shader runtime API, LPIR/native ABI, and
engine-facing shader services.

