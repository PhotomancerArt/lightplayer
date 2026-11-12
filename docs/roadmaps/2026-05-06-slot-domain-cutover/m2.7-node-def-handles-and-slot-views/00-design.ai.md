# M2.7 Design - Node Def Handles And Slot Views

## Scope

This milestone makes authored node definitions the resolver-visible default
source for consumed slots.

In scope:

- Promote the closed set of authored node definitions into a canonical
  `lpc-model::nodes::NodeDef` enum.
- Make `ArtifactStore` concrete over `NodeDef` instead of generic over an
  arbitrary payload.
- Add `NodeDefHandle` so runtime node instances identify their authored
  definition as an artifact plus a slot path.
- Make unbound `ConsumedSlot` resolution read authored def slots instead of
  runtime state slots.
- Add a small read-only `SlotView` pattern and use `TextureNode` as the first
  real runtime proof.

Out of scope:

- Full generated `SlotView` derive/codegen.
- Inline node definitions inside another artifact.
- Client/server mutation of authored defs.
- Full shader and fixture config migration.
- Broader artifact ownership cleanup beyond what is required for resolver
  fallback.

## File Structure

```text
lp-core/lpc-model/src/
  nodes/
    node_def.rs                 # canonical NodeDef enum
    mod.rs                      # exports NodeDef near node defs
    project/project_def.rs
    texture/texture_def.rs
    shader/shader_def.rs
    output/output_def.rs
    fixture/fixture_def.rs
  node/
    mod.rs                      # stops re-exporting an old trait shape if possible

lp-core/lpc-engine/src/
  artifact/
    artifact_store.rs or artifact_manager.rs
    artifact_entry.rs           # concrete ArtifactEntry
    artifact_state.rs           # concrete ArtifactState<NodeDef>
    source_loader.rs            # updated or removed if no longer useful
  node/
    node_def_handle.rs          # artifact + slot path
    node_entry.rs               # stores NodeDefHandle
    node_tree.rs                # creates entries with handles
    contexts.rs                 # typed consumed-slot helper
  engine/
    engine.rs                   # ArtifactStore owns NodeDef; consumed fallback reads defs
  project_runtime/
    project_loader.rs           # parses NodeDef and loads artifact payloads
    source_authoring_index.rs   # deleted, reduced, or delegated away
  slot_view/
    mod.rs
    slot_view_error.rs
    texture_def_view.rs         # manual proof of generated future API
```

File names can be adjusted to match the existing module map. Keep the
filesystem-oriented style: one domain concept per file where the concept has a
name.

## Architecture Summary

The engine already has the right high-level resolver rule:

```text
ConsumedSlot
  -> binding exists? resolve binding source
  -> no binding? ask host for default
```

The current host default is wrong: it ticks the node and reads
`runtime_state_slots()`. This milestone changes that fallback to read the
authored node definition:

```text
NodeId
  -> NodeEntry::def_handle
  -> ArtifactStore entry
  -> NodeDef slot root
  -> lookup SlotPath
  -> SlotDataAccess::Value
  -> RuntimeProduct
```

Produced slots keep their current semantics:

```text
ProducedSlot
  -> tick producer once
  -> read runtime_state_slots()
```

That creates the desired directionality:

- **Produced slots** come from runtime node state.
- **Consumed slots** come from bindings, with authored node def slots as
  defaults.

## Core Components

### `NodeDef`

`lpc-model/src/nodes/node_def.rs` becomes the one obvious place to add a core
node definition:

```rust
pub enum NodeDef {
    Project(ProjectDef),
    Texture(TextureDef),
    Shader(ShaderDef),
    Output(OutputDef),
    Fixture(FixtureDef),
}
```

It should provide:

- `kind() -> NodeKind`
- `kind_name() -> &'static str` if useful
- `SlotAccess` by delegating to the active variant
- TOML kind-dispatched parsing if the dependency direction stays clean
- serde/schema support where practical

The old `NodeDef` trait should be deleted if the enum replaces it cleanly. If a
support trait remains useful, rename it so `NodeDef` unambiguously means the
enum.

### `ArtifactStore`

`ArtifactStore` becomes the engine's concrete authored-artifact owner.

Instead of:

```rust
ArtifactStore<A>
ArtifactState<A>
ArtifactEntry<A>
```

the public engine-domain shape should be:

```rust
ArtifactStore
ArtifactState::Loaded(NodeDef)
ArtifactEntry { state: ArtifactState, ... }
```

Every artifact in this slice is an authored node definition, including the
project artifact. If future non-node artifacts appear, that is the moment to add
a broader `LoadedArtifact` enum.

### `NodeDefHandle`

`NodeDefHandle` identifies where a runtime node's authored definition lives:

```rust
pub struct NodeDefHandle {
    pub artifact: ArtifactId,
    pub path: SlotPath,
}
```

For now:

- root path means the loaded artifact root definition
- non-root paths are rejected or treated as unsupported

This keeps the model ready for inline node defs without implementing them.

### `SlotView`

`SlotView` is a read-only ergonomic layer over resolver access. It should not
own or borrow a `NodeDef` directly.

For this milestone, keep it minimal:

- a generic typed helper on `TickContext` or a small wrapper around it
- `TextureDefView` as the first manual view
- developer-friendly conversion errors

Future codegen can generate these views from `#[derive(SlotRecord)]` metadata.

### Texture Slice

`TextureNode` becomes the proof that node config comes through resolver access:

- `TextureNode::new(node_id)` no longer stores `TextureDef`
- `tick()` reads `size` through `TextureDefView`
- `TextureState` remains the runtime-produced state root
- tests prove authored default and binding override behavior

## Validation

Minimum validation for the finished milestone:

```bash
cargo test -p lpc-model
cargo test -p lpc-engine
cargo check -p lpc-model --features schema-gen
cargo clippy -p lpc-engine -p lpc-model -p lpc-source --all-targets -- -D warnings
```

Do not run full workspace cargo commands; this repo has RV32-only members.

