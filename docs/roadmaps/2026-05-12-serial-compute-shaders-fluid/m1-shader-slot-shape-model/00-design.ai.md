# M1 Design: Shader Slot Shape Model

## Scope

This milestone defines the authored shader slot shape model needed by serial
compute shaders. It is intentionally model-first: no compute shader runtime, no
fluid runtime, and no resolver merge execution.

In scope:

- Replace the narrow `ShaderParamDef` concept with general `ShaderSlotDef`.
- Add first-class `ComputeShaderDef` with `kind = "shader/compute"`.
- Support shader slot value shapes and map shapes.
- Allow shader slot defs to reference native LightPlayer value shapes such as
  `lp::fluid::Emitter`.
- Model GLSL lowering for map slots using a bounded sentinel array.
- Add native `FluidEmitter` as a semantic value shape.
- Generate deterministic GLSL header text for the supported M1 subset.
- Add TOML round-trip and header evidence tests.

Out of scope:

- Running compute shaders.
- Fluid node runtime.
- Resolver merge execution.
- Aggregate/non-leaf binding execution.
- Full type namespace design.
- Renaming `LpValue`.

## File Structure

```text
lp-core/lpc-model/src/
  node/
    kind.rs
  nodes/
    mod.rs
    node_def.rs
    fluid/
      mod.rs
      fluid_emitter.rs
    shader/
      mod.rs
      compute_shader_def.rs
      glsl_opts.rs
      shader_def.rs
      shader_header_gen.rs
      shader_slot_def.rs
      shader_slot_mapping.rs
      shader_state.rs
```

## Architecture Summary

`ShaderSlotDef` becomes the shared authored shader slot vocabulary. It replaces
the old visual-only `ShaderParamDef` rather than living beside it.

Visual shader defs keep the existing `param_defs` field name for now to minimize
unrelated churn, but its value type becomes `MapSlot<String, ShaderSlotDef>`.
Compute shader defs use the same vocabulary for consumed and produced slots.
Those are the technical terms in Rust. The authored TOML uses shorter
`consumed.*` and `produced.*` sections; the surrounding shader context already
makes "slot" clear enough.

```rust
pub struct ShaderDef {
    pub glsl_path: SourcePathSlot,
    pub render_order: RenderOrderSlot,
    pub bindings: BindingDefs,
    pub glsl_opts: GlslOpts,
    pub param_defs: MapSlot<String, ShaderSlotDef>,
}

pub struct ComputeShaderDef {
    pub glsl_path: SourcePathSlot,
    pub bindings: BindingDefs,
    pub glsl_opts: GlslOpts,
    pub consumed_slots: MapSlot<String, ShaderSlotDef>,
    pub produced_slots: MapSlot<String, ShaderSlotDef>,
}
```

`ShaderSlotDef` describes the semantic slot shape. For M1 it supports plain
value slots and map slots.

```rust
pub struct ShaderSlotDef {
    pub shape: ShaderSlotShapeDef,
    pub label: ValueSlot<String>,
    pub description: ValueSlot<String>,
    pub default: OptionSlot<LpValue>,
    pub mapping: OptionSlot<ShaderSlotMappingDef>,
}

pub enum ShaderSlotShapeDef {
    Value(ShaderValueShapeDef),
    Map(ShaderMapShapeDef),
}
```

Value shapes may reference native LightPlayer value shapes by an internal type
name such as `lp::fluid::Emitter`. This is a small M1 type reference mechanism, not
a complete namespace system.

```rust
pub enum ShaderValueShapeDef {
    Type(LpType),
    Native(ShaderNativeTypeRef), // e.g. "lp::fluid::Emitter"
}
```

Map slots are semantic maps, not arrays. The initial key type is `u32`.

```rust
pub struct ShaderMapShapeDef {
    pub key: ShaderMapKeyDef,      // U32 for M1
    pub value: ShaderValueShapeDef,
}
```

GLSL cannot represent maps directly, so the shader owns the mapping from
semantic slot shape to shader-visible ABI shape. The first strategy is a
fixed/bounded array with an id sentinel.

```rust
pub enum ShaderSlotMappingDef {
    Sentinel {
        len: u32,
        key_field: SlotName,
        empty_key: LpValue, // U32(0) for the first emitter use case
    },
}
```

For the first fluid emitter use case:

```toml
[produced.emitters]
kind = "map"
key = "u32"
value = "lp::fluid::Emitter"
mapping = { kind = "sentinel", len = 4, key = "id", empty_key = 0 }
```

The header generator lowers that to deterministic GLSL-like declarations:

```glsl
struct FluidEmitter {
    uint id;
    vec2 pos;
    vec2 dir;
    float radius;
    vec3 color;
    float velocity;
    float intensity;
};

out FluidEmitter emitters[4];
```

## Native Fluid Emitter Shape

`FluidEmitter` is a native semantic value shape in `lpc-model/src/nodes/fluid/`.
It is a complete slot value leaf, not a slot record. Its fields are internal
value structure and are produced/consumed as one logical emitter value.

The emitter collection is not a `FluidEmitterSet` wrapper in M1. The semantic
collection is a map slot:

```rust
MapSlot<u32, FluidEmitter>
```

or dynamically:

```rust
SlotShape::Map {
    key: SlotMapKeyShape::U32,
    value: Box::new(SlotShape::reference(FluidEmitter::SHAPE_ID)),
    ...
}
```

## Merge And Non-Leaf Binding Boundaries

Merge is receiver-owned, not binding-owned and not producer-owned. A produced
shader map slot owns only its shape and GLSL ABI mapping. A consumed slot later
owns the policy for multiple incoming values:

```toml
merge = "by_key" | "latest" | "error"
```

M1 does not execute merge policies, but the model and docs must not imply that
all bindings are leaf-only or that merge belongs to produced slots. Also note
that shader slot `mapping` and receiver `merge` are different concepts:
mapping adapts semantic slot shape to shader ABI, while merge combines multiple
incoming values at a receiver.

Follow-up work must add:

- aggregate/non-leaf binding resolution;
- receiver merge policy execution;
- resolver explain/probe output that reports merge behavior and conflicts.

## Type Naming

`LpValue` remains unchanged in M1. The notes record a likely future rename to
`LpsValue` or another system-level value name, but this milestone should not
mix a large rename into shader slot modeling.

## Native Shape Names

Static shape ids remain compact `SlotShapeId` values. M1 adds or formalizes a
human-readable native name convention for LightPlayer-authored shapes:

```text
lp::fluid::Emitter
```

The shape registry already supports named roots. `FluidEmitter` should register
with this native name so shader slot defs can reference it without copying its
field structure. This is intentionally smaller than a full namespace/import
system.
