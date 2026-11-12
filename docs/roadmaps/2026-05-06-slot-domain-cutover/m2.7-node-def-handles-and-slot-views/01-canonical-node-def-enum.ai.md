# Phase 1 - Canonical NodeDef Enum

## Scope Of Phase

Promote authored node definitions into one canonical enum in `lpc-model`.

In scope:

- Replace or rename the existing `NodeDef` trait in
  `lp-core/lpc-model/src/nodes/node_def.rs`.
- Add a `NodeDef` enum with variants for:
  - `Project(ProjectDef)`
  - `Texture(TextureDef)`
  - `Shader(ShaderDef)`
  - `Output(OutputDef)`
  - `Fixture(FixtureDef)`
- Implement shared behavior on the enum:
  - `kind() -> NodeKind`
  - `kind_name() -> &'static str` if useful
  - `SlotAccess` by delegating to the active variant
- Update model exports/imports.
- Update each concrete def file to stop implementing the old trait if it is no
  longer needed.

Out of scope:

- Engine artifact storage changes.
- Loader rewrite beyond import fallout needed by the enum.
- Runtime node config access.

## Code Organization Reminders

- Keep `node_def.rs` in `lpc-model/src/nodes/` so adding a core node type has a
  visible home.
- Keep the enum and headline impls near the top of the file.
- Put helper parse/probe types lower in the file if added here.
- Tests stay at the bottom.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests.
- If blocked by serde/schema design, stop and report the exact issue.
- Report changed files, validation run, and deviations.

## Implementation Details

Relevant files:

- `lp-core/lpc-model/src/nodes/node_def.rs`
- `lp-core/lpc-model/src/nodes/mod.rs`
- `lp-core/lpc-model/src/node/mod.rs`
- `lp-core/lpc-model/src/nodes/*/*_def.rs`
- `lp-core/lpc-model/src/lib.rs`

Expected changes:

- `NodeDef` becomes the enum name.
- If a support trait remains necessary, use a different name such as
  `NodeDefBody`; prefer deleting the trait if the enum replaces it cleanly.
- `NodeDef` implements `SlotAccess` by matching variants and delegating
  `shape_id()` / `data()`.
- Keep `ProjectDef` in the enum because the project artifact defines the root
  project node.

Edge cases:

- `SlotAccess` is object-safe and already implemented by derived concrete defs.
- Schema generation may need derives or manual impls depending on current
  feature gates.

## Validate

```bash
cargo test -p lpc-model
cargo check -p lpc-model --features schema-gen
```

