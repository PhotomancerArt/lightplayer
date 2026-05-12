# M1 Summary: Shader Slot Shape Model

## What Was Built

- Added `ShaderSlotDef` and `ShaderSlotMappingDef` as the general authored
  shader slot vocabulary.
- Replaced `ShaderParamDef` in the real model and mockup source shape.
- Added `ComputeShaderDef` with `kind = "shader/compute"` and
  `consumed` / `produced` authored slot maps.
- Added native `FluidEmitter` as `lp::fluid::Emitter`.
- Added named slot-shape lookup in `SlotShapeRegistry`.
- Added deterministic compute shader header generation for the M1 subset.
- Added tests for TOML parsing, native shape refs, sentinel mapping, and header
  evidence.

## Decisions For Future Reference

#### Native Shape Names

- **Decision:** Use explicit names such as `lp::fluid::Emitter`.
- **Why:** They are clear, Rust-like, and avoid copying native value structure
  into shader artifacts.
- **Rejected alternatives:** `lp:FluidEmitter`, implicit module-path names.
- **Revisit when:** External/shared type namespaces become necessary.

#### Shader Mapping

- **Decision:** Use inline `mapping = { kind = "sentinel", ... }` for shader ABI
  lowering.
- **Why:** `mapping` names the semantic-slot-to-shader-ABI boundary without
  sounding like GLSL source code.
- **Rejected alternatives:** `glsl`, `abi`, `sentinel_array`.
- **Revisit when:** A second mapping strategy is implemented.

#### Merge Ownership

- **Decision:** Merge policy belongs to the receiver/consumed slot, not produced
  shader slots or individual bindings.
- **Why:** Multiple bindings converge at one target, so the receiver needs one
  authoritative conflict policy.
- **Rejected alternatives:** Binding-owned merge policy, producer-owned merge
  policy.
- **Revisit when:** Aggregate binding resolution is implemented.

