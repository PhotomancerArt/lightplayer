# M3 Compute Shader Node Notes

## Goal

Add the real runtime node for `kind = "shader/compute"` artifacts.

The node should execute serial compute shaders once per tick, resolve consumed
values through the resolver, and expose produced values through the runtime slot
state system. `lp-shader` remains ABI-only: it reads/writes GLSL globals and
does not know about slot maps, merge semantics, or LightPlayer model concepts.

## Current Context

- `ComputeShaderDef` already exists in `lpc-model` and is parsed by `NodeDef`.
- M2 added `CompileComputeDesc`, `ComputeAbi`, and `LpsComputeShader`.
- `compute_desc_from_model_def` lowers authored shader slot defs to the shader
  ABI.
- Runtime produced slots are resolved by reading `NodeRuntime::runtime_state_slots`.
- Dynamic runtime state can be represented with `SlotData`, including
  `SlotData::Map(SlotMapDyn)`.
- Static/native value shapes such as `lp::fluid::Emitter` are registered in the
  shared `SlotShapeRegistry`.

## Constraints

- Do not teach `lp-shader` about maps. Sentinel arrays are shader ABI detail;
  map materialization belongs in the engine/model layer.
- Keep compute outputs as slot values/maps, not products.
- Avoid fluid-node work in this milestone.
- Keep validation targeted; do not run the full workspace host build.

## Key Questions Resolved For This Milestone

- Produced map slots are materialized from `LpsValueF32::Array` using
  `ShaderSlotMappingDef::sentinel`.
- Only `u32` map keys are required now.
- Consumed map slots remain unsupported.
- Compute node runtime state is dynamic because produced slots are authored per
  shader artifact.
- The first integration test can prove the model by resolving
  `produced.emitters` from a compute shader into a slot map.
