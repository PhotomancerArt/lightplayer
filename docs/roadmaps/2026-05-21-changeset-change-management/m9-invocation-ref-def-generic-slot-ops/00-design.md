# M9 Design — Invocation Ref|Def + Generic Slot Edit Ops

## Scope

Make child-node references honest slot data and make the edit language fully generic.

**In:** `lpc-model` invocation reshape, `VariantSet`, registry apply/diff cleanup,
in-repo tests/fixtures, `examples/`, `change-language.md`.

**Out:** `lpc-wire`, `lpa-server` client protocol, parent engine M6 cutover.

## TOML wire

```toml
# Project — external def
[nodes.shader]
ref = "./shader.toml"

# Project — inline def
[nodes.clock.def]
kind = "Clock"

# Playlist — external
[entries.2.node]
ref = "./active.toml"

# Playlist — inline
[entries.2.node.def]
kind = "Shader"
source = { path = "active.glsl" }
```

Reject: `def = { path = ... }`, `artifact = ...`.

## Rust model

```rust
pub enum NodeInvocation {
    Ref(ArtifactSpecifier),
    Def(NodeDef),
}
```

- **`Slotted` enum** — generic `set_slot_variant_default` / `set_slot_value` apply
- **Remove:** `NodeDefRef`, `def_slot`, `NODE_INVOCATION_CODEC_ID` whole-record custom path
- **Helpers:** `ref_specifier()`, `inline_def()` → match on `Ref` / `Def`

## Edit ops (layer 1)

```rust
VariantSet { path: SlotPath, variant: String }  // enum switch incl. root kind, Ref/Def
SetSlot { path: SlotPath, value: LpValue }      // value leaves only
// MapInsert, MapRemove, OptionSet unchanged
```

Apply = thin dispatch to `lpc-model` slot mutation only.

## Slot path examples

| Intent | Path / ops |
|--------|------------|
| Root kind | `VariantSet(root(), "Clock")` |
| Wire child to file | `VariantSet(nodes.shader, "Ref")` + `SetSlot(nodes.shader.ref, "./x.toml")` |
| Inline child kind | `VariantSet(nodes.clock, "Def")` + `VariantSet(nodes.clock.def, "Clock")` |
| Inline field | `SetSlot(nodes.clock.def.Clock.controls.rate, 2.0)` |
| Playlist inline patch | `SetSlot(entries[2].node.def.Shader...., ...)` |

Exact paths follow slotted enum field names (`ref`, `def`, variant names).

## File structure

```
lp-core/lpc-model/src/
├── node/
│   ├── node_invocation.rs          # REWRITE: Ref|Def slotted enum + TOML
│   └── mod.rs                      # drop NodeDefRef export
├── nodes/project/project_def.rs    # tests: ref= wire
├── nodes/playlist/playlist_entry.rs
├── slot_codec/custom_slot_codec.rs # remove invocation custom branch (if unused)
└── lib.rs

lp-core/lpc-node-registry/src/
├── edit/edit_op.rs                 # + VariantSet
├── registry/slot_apply.rs          # thin apply; delete shortcuts
├── registry/def_shell.rs           # match Ref|Def
├── registry/def_walker.rs          # match Ref|Def
├── registry/effective_read.rs      # match Ref|Def
├── registry/node_def_registry.rs   # register_invocations match
├── diff/def_diff.rs                # VariantSet; drop CustomDef
└── tests/ + harness/fixtures.rs    # new TOML

lp-core/lpc-engine/src/
├── engine/project_loader.rs        # if NodeDefRef references remain
└── ...                             # grep NodeDefRef / def_locator / inline_def

examples/**/project.toml, playlist.toml, …   # phase 07

docs/roadmaps/.../change-language.md         # phase 07
```

## Architecture

```text
NodeDef (enum slot) — artifact root
  └─ Project.nodes: Map<String, NodeInvocation>
       └─ NodeInvocation (enum slot)
            ├─ Ref → locator string leaf
            └─ Def → NodeDef enum (inline body)

EditBatch → apply → set_slot_variant_default | set_slot_value
                 (no registry domain knowledge)
```

## Validation (full gate)

```bash
cargo test -p lpc-model
cargo test -p lpc-node-registry
cargo test -p lpc-engine
cargo test -p fw-tests --test scene_render_emu --no-run
cargo clippy -p lpc-model -p lpc-node-registry -p lpc-engine --all-targets --no-deps -- -D warnings
just check   # phase 08
```
