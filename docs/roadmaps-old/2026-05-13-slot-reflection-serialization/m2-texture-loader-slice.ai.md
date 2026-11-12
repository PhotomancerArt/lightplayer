# M2 Texture Loader Slice

Date: 2026-05-13

## Goal

Put one real production node artifact on the native slot TOML path while
keeping the scope small enough to review carefully.

## What Landed

`ProjectLoader` now routes `kind = "texture"` child node artifacts through the
slot-shape-driven TOML codec instead of `NodeDef::from_toml_str`.

The load path is:

1. Parse TOML into `toml::Value`.
2. Read top-level `kind` as loader metadata.
3. For `texture`, decode the table into `SlotData` using `TextureDef`'s
   `SlotShape`.
4. Hydrate the typed `TextureDef` from that `SlotData`.
5. Other node kinds continue through the existing Serde-backed loader.

This is intentionally not a broad loader rewrite yet.

## Authored Binding Compatibility

The first production slice exposed an important modeling issue: `BindingDef`
was represented as a slot value leaf whose `LpValue` shape was a struct with
`direction` and `endpoint`, while the authored TOML shape uses:

```toml
[bindings.input]
source = "bus#visual.out"

[bindings.output]
target = "bus#visual.out"
```

Adding custom `BindingDef` logic to the authored TOML codec worked, but it
violated the core rule for this experiment: persisted domain data should be in
the slot system, not hidden in format-specific code.

The fix was to move the shape into the model:

- `BindingEndpoint` is now a semantic string slot leaf.
- `BindingDef` is now a static slot root/record with:
  - `source: OptionSlot<ValueSlot<BindingEndpoint>>`
  - `target: OptionSlot<ValueSlot<BindingEndpoint>>`
- `BindingDefs` maps slot names to `BindingDef` via a `SlotShape::Ref`.

With that shape, the generic TOML record/option/value rules naturally decode
the authored `source = "..."` and `target = "..."` syntax. `lpc-wire` no
longer contains any `BindingDef`-specific serialization logic.

## Current Limits

- Only `TextureDef` uses the production native path.
- The texture hydrator is handwritten inside `ProjectLoader`; it should move or
  become generated before more roots are added.
- `ProjectDef`, `ShaderDef`, `OutputDef`, and `FixtureDef` still use typed
  Serde/TOML loading.
- The top-level `NodeDef` probe still uses TOML parsing, but not typed
  per-root Serde for texture.
- Texture TOML must use the current `size = { width, height }` or `[size]`
  shape. Older `width`/`height` top-level files are not part of this slice.
- Native slot decoding models `BindingEndpoint` as a compact string leaf, so
  literal source bindings remain a Serde-only compatibility path for now.

## Validation

Focused validation passed:

```bash
cargo test -p lpc-engine engine::project_loader::tests
cargo test -p lpc-model binding
cargo test -p lpc-source --test basic_example_parse
cargo test -p lpc-wire --test source_slot_sync
cargo test -p lpc-slot-mockup
cargo check -p lpc-wire --no-default-features
cargo check -p lpc-engine --no-default-features
```
