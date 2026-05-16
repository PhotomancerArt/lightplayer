# M3 Switch Definition Loading Design

## Scope

M3 makes authored node definition TOML load through SlotCodec. It keeps the
public authored TOML concept of `kind`, but treats it as wrapper metadata around
the concrete slotted record.

## File Structure

```text
lp-core/
  lpc-model/src/nodes/
    node_def.rs                       # NodeDef authored TOML SlotCodec reader/writer helpers
    project/project_def.rs            # tests migrate away from serde TOML reads
    texture/texture_def.rs            # tests migrate away from serde TOML reads
    shader/shader_def.rs              # add coverage if needed
    output/output_def.rs              # tests migrate away from serde TOML reads
    fixture/fixture_def.rs            # add coverage for enum/value leaf pressure

  lpc-engine/src/engine/
    project_loader.rs                 # pass registry into NodeDef TOML loading

  lpc-shared/src/project/
    builder.rs                        # authored fixture writer uses SlotCodec TOML for slotted payloads
```

## Architecture Summary

Authored definition loading should become:

```text
project.toml / node.toml text
    |
    v
toml::Value
    |
    v
NodeDef wrapper reader consumes `kind`
    |
    v
SlotShapeRegistry::read_slot_toml(concrete_shape_id, payload_table)
    |
    v
Box<dyn SlotMutAccess>
    |
    v
downcast concrete def
    |
    v
NodeDef::<variant>
```

This keeps discriminators explicit without making `kind` a field on
`ProjectDef`, `TextureDef`, `ShaderDef`, `OutputDef`, or `FixtureDef`.

## Main Components

### NodeDef Authored TOML Reader

`NodeDef::from_toml_str` should stop using serde for model payloads. It can
either be replaced or kept as a compatibility-named entry point that requires a
registry.

Expected helper shape:

```rust
impl NodeDef {
    pub fn from_toml_str_with_registry(
        registry: &SlotShapeRegistry,
        text: &str,
    ) -> Result<Self, NodeDefParseError>;
}
```

The implementation should parse `toml::Value`, extract `kind`, clone/remove it
from the table payload, call `registry.read_slot_toml`, and downcast the result.

### Project Loader Boundary

`ProjectLoader` already has access to `runtime.slot_shapes()` after constructing
the engine. The loading order may need a small reshape so project and child defs
are decoded using the engine's registry instead of a static serde function.

### Authored TOML Writer Helper

For generated authored test projects, add a helper that writes:

```text
kind = "<domain-kind>"
<SlotCodec TOML payload>
```

This can initially live in `lpc-shared/src/project/builder.rs` unless it becomes
useful enough to move into `lpc-model::nodes::node_def`.

### Syntax Compatibility

M3 should preserve existing authored TOML where practical:

- `kind = "project"` stays lower-case
- project child nodes stay under `[nodes.<name>]`
- enum payloads continue using explicit `kind` discriminators
- unknown fields stay errors

Any syntax difference should be captured in `future.md` or `summary.md` with a
specific before/after example.

## Non-Goals

- Do not remove serde derives.
- Do not remove serde from wire envelopes.
- Do not invent schema versioning.
- Do not make TOML loading streaming in M3.
