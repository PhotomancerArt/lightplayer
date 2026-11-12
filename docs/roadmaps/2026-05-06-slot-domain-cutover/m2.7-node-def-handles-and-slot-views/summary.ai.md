# M2.7 Summary - Node Def Handles And Slot Views

## What Was Built

- Added canonical `lpc_model::nodes::NodeDef` enum with `Project`, `Texture`, `Shader`, `Output`, and `Fixture` variants.
- Centralized TOML kind dispatch in `NodeDef::from_toml_str`.
- Made `ArtifactStore` concrete over loaded `NodeDef` payloads and removed the generic artifact payload surface.
- Added `NodeDefHandle` and stored it on `NodeEntry`.
- Loaded project and child node definitions into `ArtifactStore`.
- Removed `SourceAuthoringIndex` and the engine-local `LoadedNodeDef` enum.
- Changed unbound consumed-slot fallback to read authored `NodeDef` slots through the node's `NodeDefHandle`.
- Added a minimal resolver-backed `TextureDefView`.
- Ported `TextureNode` so it no longer owns `TextureDef`; it reads authored/bound `size` through the resolver during tick.

## Decisions For Future Reference

#### Canonical NodeDef Enum

- **Decision:** `NodeDef` is a closed enum in `lpc-model/src/nodes/node_def.rs`.
- **Why:** The loader already had a hard-coded kind branch; moving it into one model-level enum makes adding a core node kind explicit and searchable.
- **Rejected alternatives:** A dyn-trait artifact payload for core node defs.
- **Revisit when:** Plugin-defined or externally supplied node kinds become a real requirement.

#### ArtifactStore Owns NodeDef

- **Decision:** `ArtifactStore` is no longer generic and stores `NodeDef` payloads.
- **Why:** In the current domain, every loaded artifact is an authored node definition, including the project artifact.
- **Rejected alternatives:** A separate authoring-def store; `ArtifactStore<A>` as public engine infrastructure.
- **Revisit when:** Non-node artifacts need to share the same artifact lifecycle.

#### Consumed Defaults Come From Authored Defs

- **Decision:** Unbound consumed slots fall back to the node's authored `NodeDef` slot root.
- **Why:** Bindings should override authored config, and nodes should not read copied config directly.
- **Rejected alternatives:** Reading unbound consumed slots from runtime state.
- **Revisit when:** Inline node defs require non-root `NodeDefHandle` paths.

#### SlotView Is Read-Only

- **Decision:** The first `SlotView` surface is read-only and delegates to `TickContext`.
- **Why:** Runtime nodes should consume config through the resolver; mutations belong on future message APIs.
- **Rejected alternatives:** Borrowing node defs directly from the artifact store inside nodes.
- **Revisit when:** Generated typed views are added.

