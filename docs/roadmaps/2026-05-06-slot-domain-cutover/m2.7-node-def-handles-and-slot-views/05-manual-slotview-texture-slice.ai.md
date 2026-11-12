# Phase 5 - Manual SlotView Texture Slice

## Scope Of Phase

Add the first read-only typed view and port `TextureNode` to use it.

In scope:

- Add a minimal `SlotView` helper pattern.
- Add a manual `TextureDefView`.
- Change `TextureNode` so it no longer stores `TextureDef`.
- `TextureNode::tick()` reads `size` through the view/resolver.
- Preserve `TextureState` as the produced runtime state root.
- Add tests showing authored defaults and binding override affect texture
  runtime state.

Out of scope:

- Generated `SlotView` derive/codegen.
- Shader and fixture config migration.
- Mutable views.
- Client-side typed views.

## Code Organization Reminders

- Prefer a small `slot_view/` module if this becomes more than one file.
- Keep the manual view obvious enough that future codegen requirements are
  visible.
- Do not hide field-specific logic in a large generic helper if only texture
  needs it in this phase.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope to shader/fixture.
- Do not bypass resolver by borrowing `TextureDef` directly.
- Report conversion gaps in `LpValue` instead of papering over them.

## Implementation Details

Relevant files:

- `lp-core/lpc-engine/src/node/contexts.rs`
- `lp-core/lpc-engine/src/nodes/texture/texture_node.rs`
- `lp-core/lpc-engine/src/nodes/texture/mod.rs`
- new `lp-core/lpc-engine/src/slot_view/*` if useful
- `lp-core/lpc-model/src/slots/dim2u.rs`
- `lp-core/lpc-model/src/slot/slot_value.rs`

Possible API:

```rust
impl TickContext<'_> {
    pub fn resolve_consumed_slot_value<T>(&mut self, slot: &SlotPath) -> Result<T, NodeError>
    where
        T: FromLpValue,
}
```

or:

```rust
pub struct TextureDefView<'a, 'ctx> {
    ctx: &'a mut TickContext<'ctx>,
}

impl TextureDefView<'_, '_> {
    pub fn size(&mut self) -> Result<Dim2u, NodeError>;
}
```

Use whichever shape is cleanest while preserving the invariant: the view
delegates to `TickContext::resolve(QueryKey::ConsumedSlot { node, slot })`.

Tests:

- `TextureNode::new(node_id)` has no `TextureDef` parameter.
- With no binding, tick copies authored `TextureDef.size` into `TextureState`.
- With a literal binding override, tick copies the override into `TextureState`.

## Validate

```bash
cargo test -p lpc-engine texture
cargo test -p lpc-engine
```

