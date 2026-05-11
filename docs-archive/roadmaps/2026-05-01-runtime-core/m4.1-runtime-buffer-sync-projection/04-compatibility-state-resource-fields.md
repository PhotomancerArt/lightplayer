# Phase 4: Compatibility state resource fields

## Scope of phase

Add searchable compatibility wrappers for heavy semantic state fields so node
details can point at resource refs without embedding heavy bytes as the source
of truth.

In scope:

- Add compatibility wrapper types in `lpc-wire`.
- Update legacy state fields for texture/output/fixture heavy data.
- Preserve semantic field names (`lamp_colors`, `channel_data`, etc.).
- Keep existing inline compatibility behavior where needed.

Out of scope:

- Long-term core node state redesign; M4.5 owns it.
- Engine projection implementation.
- Client cache implementation.

## Code organization reminders

- Put compatibility wrappers in a clearly named module/file.
- Include `compatibility` or `legacy` in names/docs so M4.5 can find them.
- Keep helpers at the bottom.
- Avoid changing unrelated state fields.

## Sub-agent reminders

- Do not commit.
- Do not weaken serialization tests.
- Do not remove legacy inline compatibility unless tests prove replacement.
- If the serde shape becomes too invasive, stop and report.

## Implementation details

Read:

- `00-notes.md` Q22-Q24.
- `lp-core/lpc-wire/src/legacy/nodes/fixture/state.rs`
- `lp-core/lpc-wire/src/legacy/nodes/output/state.rs`
- `lp-core/lpc-wire/src/legacy/nodes/texture/state.rs`
- `lp-core/lpc-wire/src/legacy/nodes/shader/state.rs`
- `lp-core/lpc-view/src/project/project_view.rs`

Add a compatibility wrapper for heavy data fields. A possible shape:

```rust
pub enum LegacyResourceField<T> {
    Inline(Versioned<T>),
    Resource {
        resource: ResourceRef,
        changed_frame: FrameId,
    },
}
```

Use a narrower byte-specific wrapper if that is simpler and clearer.

Apply to:

- `OutputState.channel_data`: runtime buffer ref capable;
- `FixtureState.lamp_colors`: runtime buffer ref capable;
- `TextureState.texture_data`: render product ref capable or metadata-only
  compatible;
- `FixtureState.mapping_cells`: keep inline compatibility snapshot for M4.1.

Maintain partial serialization semantics as much as possible. Existing clients
should be able to deserialize new resource-ref forms once `lpc-view` is updated.

Add tests for:

- inline field serialization still works;
- resource-ref field serialization/deserialization;
- partial merge preserves existing field when omitted;
- semantic field names remain unchanged in JSON.

## Validate

Run:

```bash
cargo test -p lpc-wire fixture
cargo test -p lpc-wire output
cargo test -p lpc-wire texture
cargo test -p lpc-view project
```
