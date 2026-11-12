# M3 Compute Shader Node Design

## Runtime Node

`ComputeShaderNode` owns:

- `node_id`
- authored `ComputeShaderDef`
- GLSL source with generated header prepended
- compiled `LpsComputeShader`
- dynamic runtime state root

On tick:

1. Resolve each consumed value slot through `TickContext`.
2. Compile the shader on first use or after authored config changes.
3. Execute `tick()` with resolved inputs.
4. Read each produced output global.
5. Convert shader outputs into `SlotData` and stamp them with the current
   revision.

## Runtime State Shape

The compute node registers one dynamic runtime state shape per node instance.

The root shape is a record whose fields are the authored produced slot names.
Each field shape is derived from `ShaderSlotDef`:

- `kind = "value"` becomes `SlotShape::Value`.
- `kind = "map"` becomes `SlotShape::Map` with the authored key shape and a
  value leaf/reference to the native value shape.

The runtime state data mirrors the same field order.

## Sentinel Map Materialization

For map outputs:

1. Read the shader global by produced slot name.
2. Require `LpsValueF32::Array`.
3. Convert each non-empty item to `LpValue`.
4. Extract the key field named by `mapping.key`.
5. Skip entries whose key equals `mapping.empty_key`.
6. Store entries in `SlotMapDyn` as `SlotData::Value(WithRevision<LpValue>)`.

The materializer is engine-owned so future merge/explain behavior can be built
around resolver and slot semantics instead of shader ABI internals.

## Graphics Boundary

`LpGraphics` grows a compute compile method returning `Box<dyn LpComputeShader>`.
Backends wrap `lp_shader::LpsComputeShader` behind a small engine trait so
nodes do not depend on concrete backend types.

## Loader

`ProjectLoader` attaches `ComputeShaderNode` for `NodeDef::ComputeShader` and
registers bindings for authored consumed/produced slots.

For this milestone, direct consumed/default resolution is enough. Bus binding
for produced compute maps may land if it falls out cleanly, but fluid merge
semantics are out of scope.
