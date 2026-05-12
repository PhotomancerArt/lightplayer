# Phase 2: Node Runtime State Root

## Scope Of Phase

Add the node runtime state slot-root accessor.

In scope:

- Add an empty runtime state slot root.
- Add `NodeRuntime::runtime_state_slots() -> &dyn SlotAccess` or similar.
- Preserve existing opaque `RuntimeStateAccess` only if required by old callers; otherwise rename or retire it.
- Add tests showing a dummy node can expose a slot root.

Out of scope:

- Converting shader.
- Generic node sync.
- UI/watch behavior.

## Implementation Details

Relevant files:

- `lp-core/lpc-engine/src/node/node_runtime.rs`
- `lp-core/lpc-engine/src/prop/mod.rs`
- `lp-core/lpc-engine/src/prop/produced_slot_access.rs`

The empty state root can be a simple unit or empty record with a stable shape id:

```rust
SlotShapeId::from_static_name("engine.empty_state")
```

## Validate

```bash
cargo test -p lpc-engine node::
cargo check -p lpc-engine
```

