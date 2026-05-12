# Phase 6 - Cleanup And Validation

## Scope Of Phase

Clean up temporary compatibility pieces and validate the milestone.

In scope:

- Remove stale `LoadedNodeDef` exports from engine.
- Remove or clarify `SourceAuthoringIndex` if it remains.
- Update rustdocs for:
  - `NodeDef`
  - `ArtifactStore`
  - `NodeDefHandle`
  - consumed-slot default resolution
  - `SlotView`
- Ensure errors are developer-friendly.
- Run final validation commands.

Out of scope:

- Full roadmap cleanup beyond this milestone.
- Shader/fixture migration.
- Wire/view mutation.

## Code Organization Reminders

- Remove empty modules rather than leaving placeholders.
- Keep TODOs only when they name explicit future work.
- Avoid broad refactors that are not needed for this milestone.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings.
- If validation exposes unrelated failures, report them with evidence.

## Implementation Details

Audit for stale names:

```bash
rg "LoadedNodeDef|ArtifactStore<|ArtifactManager|SourceAuthoringIndex|NodeDef trait|runtime_state.*ConsumedSlot"
```

Expected final shape:

- `lpc-model::nodes::NodeDef` is the canonical enum.
- `ArtifactStore` owns `NodeDef`.
- `NodeEntry` has a `NodeDefHandle`.
- `ConsumedSlot` fallback reads authored defs.
- `TextureNode` uses resolver-backed config access.

## Validate

```bash
cargo fmt --check
cargo test -p lpc-model
cargo test -p lpc-engine
cargo check -p lpc-model --features schema-gen
cargo clippy -p lpc-engine -p lpc-model -p lpc-source --all-targets -- -D warnings
```

