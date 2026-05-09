# Phase 2: TickContext And TextureDefView

## Scope

Use compiled accessors for the current texture-definition view slice.

Out of scope:

- Generated views for every def type.
- Accessor-aware resolver query keys.

## Implementation Details

Update `TickContext` with:

```rust
pub fn resolve_consumed_slot_accessor_value<T>(
    &mut self,
    accessor: &SlotAccessor,
) -> Result<T, NodeError>
where
    T: FromLpValue;
```

This can initially delegate through the existing `QueryKey::ConsumedSlot` using `accessor.path()` while preserving the compiled handle API for view callers.

Update `TextureDefView` so it is compiled:

```rust
pub struct TextureDefView {
    size: SlotAccessor,
}
```

It should compile `size` against `TextureDef::SHAPE_ID` and expose:

```rust
pub fn compile(registry: &SlotShapeRegistry) -> Result<Self, SlotAccessorError>;
pub fn size(&self, ctx: &mut TickContext) -> Result<Dim2u, NodeError>;
```

Update `TextureNode::tick` to use the compiled/generated-compatible view. If there is no cache yet, compiling inside tick is acceptable as an intermediate step only if the accessor itself checks registry revision. Prefer storing the view on `TextureNode` if the borrow structure stays clean.

Update existing texture tests to prove behavior remains the same.

## Validate

```bash
cargo test -p lpc-engine texture
```

