# Phase 3: Node Call And Render Capability

## Scope Of Phase

Add the node-call vocabulary, explicit executing state, and render capability trait.

In scope:

- Add `NodeCall` / `NodeCallKey`.
- Add `NodeEntryState::Executing { call: NodeCallKey }`.
- Add `RenderNode` capability.
- Add a default `NodeRuntime::render_node() -> Option<&mut dyn RenderNode>`.
- Update engine dispatch to use `Executing` instead of temporarily replacing active nodes with `Pending`.
- Produce clear errors for attempted calls into executing nodes.

Out of scope:

- Allowing same-node re-entry.
- Broad async/coroutine machinery.
- Moving shader compile/render state.
- Removing render materialization from the old render product trait path, unless it falls out naturally.

## Code Organization Reminders

- Put `NodeCall` / `NodeCallKey` in a dedicated file such as `node/node_call.rs`.
- Put `RenderNode` in a dedicated file such as `node/render_node.rs`.
- Keep helpers lower in files; headline traits/types should be near the top.
- Keep `Executing` docs clear: it is an active call state, not a lifecycle failure.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-core/lpc-engine/src/node/node_entry_state.rs`
- `lp-core/lpc-engine/src/node/node_runtime.rs`
- `lp-core/lpc-engine/src/node/mod.rs`
- `lp-core/lpc-engine/src/node/contexts.rs`
- `lp-core/lpc-engine/src/engine/engine.rs`
- `lp-core/lpc-engine/src/engine/engine_error.rs`

Expected changes:

- Add:

```rust
pub enum NodeCall {
    Tick,
    ProduceSlot { slot: SlotPath },
    Render { product: RenderProduct },
}

pub struct NodeCallKey {
    pub node: NodeId,
    pub call: NodeCall,
}
```

- Add:

```rust
pub trait RenderNode {
    fn render_texture(
        &mut self,
        product: RenderProduct,
        request: &RenderTextureRequest,
        ctx: &mut RenderContext<'_>,
    ) -> Result<TextureRenderProduct, NodeError>;
}
```

- Add to `NodeRuntime`:

```rust
fn render_node(&mut self) -> Option<&mut dyn RenderNode> {
    None
}
```

- Add `NodeEntryState::Executing { call: NodeCallKey }`.
- Change engine node-calling code so stolen nodes leave `Executing`, not `Pending`.
- If the engine attempts to call an executing node, return a specific error. The error should include the active call and attempted call where practical.

Validation focus:

- Existing node tree sync tests may need update for wire conversion of `Executing`. If wire has no equivalent yet, map it to `Alive` only if the state is never externally observed; otherwise add a wire state intentionally.
- Add tests for `Executing` discriminants and re-entry diagnostics.

## Validate

```bash
cargo check -p lpc-engine
cargo test -p lpc-engine node::
cargo test -p lpc-engine engine::
cargo test -p lpc-engine resolver::
```

