# Phase 5: Produced Slot Cleanup

## Scope Of Phase

Reduce node produced-slot boilerplate where the node itself naturally owns the produced values.

In scope:

- Implement `ProducedSlotAccess` directly on `ShaderNode`.
- Remove `ShaderProducedSlots` if it is no longer needed.
- Opportunistically clean similarly simple sidecar produced-slot structs only when low-risk.
- Keep sidecar structs where they clearly reduce complexity.

Out of scope:

- Macro/codegen for produced slots.
- Full node state/slot exposure conversion.
- Fixture/output product redesign.

## Code Organization Reminders

- Put the node’s produced-slot implementation near the node implementation.
- Avoid creating more tiny sidecar structs for one-slot outputs unless they are genuinely reusable.
- Keep tests at the bottom of files.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-core/lpc-engine/src/nodes/shader/shader_node.rs`
- `lp-core/lpc-engine/src/nodes/texture/texture_node.rs`
- `lp-core/lpc-engine/src/nodes/fixture/fixture_node.rs`
- `lp-core/lpc-engine/src/nodes/output/output_node.rs`
- `lp-core/lpc-engine/src/engine/test_support.rs`

Expected changes:

- `ShaderNode` should implement `ProducedSlotAccess` directly:

```rust
impl ProducedSlotAccess for ShaderNode {
    fn get(&self, path: &SlotPath) -> Option<(RuntimeProduct, Revision)> { ... }
    fn iter_changed_since<'a>(...) -> Box<dyn Iterator<...> + 'a> { ... }
    fn snapshot<'a>(...) -> Box<dyn Iterator<...> + 'a> { ... }
}
```

- `NodeRuntime for ShaderNode` should return `self` from `produced()`.
- If texture node still has simple produced metadata and cleanup is easy, consider direct implementation there too. Do not force it.
- Keep tests meaningful: they should assert the node’s produced access exposes expected products/values.

## Validate

```bash
cargo check -p lpc-engine
cargo test -p lpc-engine nodes::shader
cargo test -p lpc-engine nodes::texture
cargo test -p lpc-engine prop::produced_slot_access
```

