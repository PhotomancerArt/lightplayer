# M2.6 Runtime State Slot Roots Design

## Scope

Make runtime node state the public slot-root surface for produced outputs. This replaces the
separate produced-root concept with owned node state records.

## File Structure

```text
lp-core/lpc-model/src/
  value/
    lp_value.rs              # add LpValue::RenderProduct
    lp_type.rs               # add LpType::RenderProduct
  resource/
    render_product.rs        # graph render product value
  slots/
    render_product.rs        # RenderProductSlot semantic leaf

lp-core/lpc-engine/src/
  render_product/
    render_product.rs        # removed/re-export model RenderProduct
  node/
    node_runtime.rs          # add runtime state slot root accessor
  nodes/
    shader/
      shader_node.rs
      shader_state.rs        # ShaderState slot root
  prop/
    produced_slot_access.rs  # temporary bridge, then shrink/delete later
```

Exact filenames may adjust, but keep the "filesystem as map" style.

## Architecture Summary

`RenderProduct` becomes a model value:

```rust
pub struct RenderProduct {
    node: NodeId,
    output: u32,
}

pub enum LpValue {
    ...
    RenderProduct(RenderProduct),
}
```

The engine still has `RuntimeProduct`, but its render variant carries the model type:

```rust
pub enum RuntimeProduct {
    Value(LpsValueF32),
    Render(RenderProduct),
    Buffer(RuntimeBufferId),
}
```

Shader runtime state is a normal slot root:

```rust
#[derive(lpc_model::SlotRecord)]
#[slot(root)]
pub struct ShaderState {
    pub output: RenderProductSlot,
}
```

`ShaderNode` owns this public state:

```rust
pub struct ShaderNode {
    node_id: NodeId,
    config: ShaderDef,
    glsl_source: String,
    shader: Option<Box<dyn LpShader>>,
    compilation_error: Option<String>,
    state: ShaderState,
}
```

`NodeRuntime` exposes the public runtime state:

```rust
fn runtime_state_slots(&self) -> &dyn SlotAccess {
    &EMPTY_RUNTIME_STATE_SLOTS
}
```

Produced resolution remains directional engine logic:

```text
resolve ProducedSlot(shader, "output")
  tick shader if needed
  read shader.runtime_state_slots().data at path "output"
  convert LpValue::RenderProduct to RuntimeProduct::Render
```

This keeps the state namespace generic while preserving the engine-specific meaning of "produced".

## Resolution Strategy

M2.6 reads produced values through runtime state slots using the engine's slot shape registry:

```rust
fn lookup_slot_data(
    node.runtime_state_slots(),
    engine.slot_shapes(),
    path,
) -> Result<SlotDataAccess, SlotLookupError>
```

The resolver then:

- converts `LpValue::RenderProduct` to `RuntimeProduct::Render`,
- converts scalar/vector `LpValue` values to `RuntimeProduct::Value` for existing shader-value tests,
- reports unresolved produced/consumed slots when the state path is missing or not a value.

`ProducedSlotAccess` is removed in this milestone; the runtime state slot root is the single node
surface for produced values.

## Main Interactions

Shader output:

```text
ShaderNode::new
  state.output = RenderProductSlot::new(RenderProduct::new(node_id, 0))

ShaderNode::tick
  state.output.set(RenderProduct::new(node_id, 0)) or mark revision

Engine resolves shader output
  produced_get(shader, "output")
  -> RuntimeProduct::Render(RenderProduct { node: shader_id, output: 0 })
```

Fixture input:

```text
FixtureNode resolves input binding
  gets RuntimeProduct::Render(...)
  asks TickContext to render texture
  EngineSession dispatches to RenderNode on product.node
```
