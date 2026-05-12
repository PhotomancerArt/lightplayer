# Phase 3: Shader State Output

## Scope Of Phase

Convert `ShaderNode.output` to `ShaderState.output`.

In scope:

- Add `nodes/shader/shader_state.rs`.
- Define `ShaderState` as `#[derive(SlotRecord)] #[slot(root)]`.
- Add `state: ShaderState` to `ShaderNode`.
- Remove direct hand-written `ProducedSlotAccess` from `ShaderNode`.
- Update shader tests to assert state shape and output value.

Out of scope:

- Compile error state, unless it is trivial and does not distract from output.
- Dynamic shader params.
- Runtime editability.

## Implementation Details

`ShaderState` should start with one field:

```rust
#[derive(lpc_model::SlotRecord)]
#[slot(root)]
pub struct ShaderState {
    pub output: RenderProductSlot,
}
```

`ShaderNode::new` initializes:

```rust
ShaderState::new(RenderProduct::new(node_id, 0))
```

`ShaderNode::tick` marks `output` with the current revision so existing produced-slot semantics
continue to observe a fresh output each frame.

## Validate

```bash
cargo test -p lpc-engine nodes::shader::
cargo test -p lpc-engine engine::
```

