# Phase 3 — Move Source Model to `lpc-source`

## Scope of phase

Move authored/on-disk source model concepts out of `lpc-model` into
`lpc-source`, using `Src*` names for ambiguous persisted source types and
preserving existing serialization behavior.

Out of scope:

- Do not move wire protocol/message/tree-delta types; that is Phase 4.
- Do not move engine runtime types; that is Phase 5.
- Do not introduce new source behavior beyond what exists today.
- Do not make `lpc-source` depend on `lps-shared`.
- Do not commit.

## Code organization reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests first.
- Place helper utility functions at the bottom of files.
- Keep related functionality grouped together.
- Any temporary code should have a `TODO` comment so it can be found later.

## Sub-agent reminders

- Do not commit. The plan commits at the end as a single unit.
- Do not expand scope. Stay strictly within this phase.
- Do not suppress warnings or `#[allow(...)]` problems away; fix them.
- Do not disable, skip, or weaken existing tests to make the build pass.
- If blocked by a source/wire boundary ambiguity, stop and report.
- Report back: files changed, validation run, validation result, and any
  deviations from this phase.

## Implementation details

Move these source/on-disk concepts from `lp-core/lpc-model/src` to
`lp-core/lpc-source/src`:

- `artifact/`
- `node/node_config.rs`
- `prop/binding.rs`
- `prop/shape.rs`
- `value_spec.rs`
- `presentation.rs`
- `schema/mod.rs`

The target structure should be granular:

```text
lp-core/lpc-source/src/
├── lib.rs
├── artifact/
│   ├── mod.rs
│   ├── artifact.rs
│   ├── artifact_spec.rs
│   └── load_artifact.rs
├── node/
│   ├── mod.rs
│   └── src_node_config.rs
├── prop/
│   ├── mod.rs
│   ├── src_binding.rs
│   ├── src_shape.rs
│   ├── src_slot.rs
│   ├── src_value_spec.rs
│   ├── src_value_spec_wire.rs
│   ├── src_texture_spec.rs
│   └── toml_parse.rs
├── presentation.rs
└── schema/
    ├── mod.rs
    ├── migration.rs
    └── registry.rs
```

Use practical file names if exact splitting becomes awkward, but do not keep a
single 1500-line `value_spec.rs`-style file. The user explicitly prefers
single-concept files.

### Naming

Use `Src*` names where the type would otherwise be ambiguous:

- `NodeConfig` -> `SrcNodeConfig` if call-site churn is manageable.
- `Binding` -> `SrcBinding`.
- `Shape` -> `SrcShape`.
- `Slot` -> `SrcSlot`.
- `ValueSpec` -> `SrcValueSpec`.
- `TextureSpec` -> `SrcTextureSpec`.

If a broad rename causes too much churn in this phase, keep the old public name
as a short-term re-export in `lpc-source`, but the primary type should be the
`Src*` name. Do not add compatibility re-exports from `lpc-model`.

### Reuse existing `ValueSpec` wire machinery

Do not duplicate the current private `LpsValueWire` shape.

Instead:

- Replace the private `LpsValueWire` concept with `lpc_model::WireValue`.
- Keep the existing internally tagged value spec serde shape:

```rust
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
enum SrcValueSpecWire {
    Literal(WireValue),
    Texture(SrcTextureSpec),
}
```

- `SrcValueSpec::Literal` should contain `WireValue`, not `LpsValue`.
- `SrcValueSpec::Texture` should contain `SrcTextureSpec`.
- Preserve current JSON/TOML round-trip behavior as much as possible.

### Materialization

Any method that materializes a source value into `LpsValue` no longer belongs
in `lpc-source` if it requires `lps-shared`.

Move such logic out or leave a source-only representation that can be converted
by `lpc-engine` in Phase 5. If moving materialization requires changing many
runtime call sites, prefer a small engine-side conversion helper in Phase 5
rather than reintroducing `lps-shared` into `lpc-source`.

### Update imports

Update immediate dependents enough to compile this phase:

- `lpc-model` should no longer export source types from crate root.
- Crates that use source types should add `lpc-source` dependencies as needed.
- Existing code that imports `lpc_model::{Artifact, Binding, Shape, Slot,
  ValueSpec, NodeConfig, Presentation}` should import from `lpc_source`.

Do not do a full workspace import sweep if it gets too broad; Phase 6 handles
global dependents. But `cargo check -p lpc-source` and the crates touched by
this phase must compile.

### Finish removing `lps-shared` from `lpc-model`

After source files move out, verify `lpc-model` no longer needs `lps-shared`.
If Phase 2 left the dependency temporarily, remove it now.

## Tests to preserve/add

- Preserve existing `ValueSpec`/source value serde tests after converting to
  `WireValue`.
- Preserve TOML parsing tests for scalar/vector/array/struct defaults.
- Preserve artifact load tests if present.
- Add one test that `SrcValueSpec::Literal(WireValue::F32(...))` serializes in
  the same tagged form as the old `ValueSpec::Literal`.

## Validate

Run:

```bash
cargo test -p lpc-source
cargo test -p lpc-model
cargo check -p lpc-model --no-default-features
cargo check -p lpc-source --no-default-features
```

If formatting changed, run:

```bash
cargo +nightly fmt
```
