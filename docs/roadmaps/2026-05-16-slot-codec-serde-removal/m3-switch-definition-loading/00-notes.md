# M3 Switch Definition Loading Notes

## Scope

Milestone 3 switches authored project/node definition loading from
Serde-owned TOML payloads to SlotCodec-owned TOML payloads.

In scope:

- replace `NodeDef::from_toml_str` serde probing and serde variant parsing
- read `project.toml` and child node artifact TOML through `SlotShapeRegistry`
- preserve the explicit authored discriminator `kind = "..."`
- update authored-definition tests so they prove SlotCodec TOML reads
- switch test/project-builder authored TOML output away from `toml::to_string`
  where it writes slotted model payloads
- document any syntax differences discovered during migration

Out of scope:

- removing serde derives from `lpc-model`
- removing `toml` as the disk parser
- switching project-read/message paths beyond the M2 slice
- schema versioning or backward compatibility policy
- broad authored format redesign

## Current State

The project loader reads authored definitions here:

- `lp-core/lpc-engine/src/engine/project_loader.rs`
  - `load_project_def`
  - `load_node_def`
  - both call `NodeDef::from_toml_str`

The current node-definition parser lives here:

- `lp-core/lpc-model/src/nodes/node_def.rs`
  - `NodeDef::from_toml_str`
  - `NodeDefKindProbe`
  - `parse_variant<T: serde::de::DeserializeOwned>`

That parser does two serde-backed operations:

1. deserialize only `kind`
2. deserialize the whole TOML text into the selected concrete def type

SlotCodec TOML already exists:

- `SlotShapeRegistry::read_slot_toml`
- `SlotShapeRegistry::write_slot_toml`
- `TomlSyntaxSource`
- `read_dynamic_slot`

The mockup already exercises registry-backed TOML reads and writes in:

- `lp-core/lpc-slot-mockup/src/tests/dynamic_slot_codec.rs`
- `lp-core/lpc-slot-mockup/src/tests/storage_codec.rs`

The authored project builder still writes TOML using serde:

- `lp-core/lpc-shared/src/project/builder.rs`
  - manually writes `project.toml`
  - writes node artifacts with `toml::to_string(&config)`
  - prepends `kind = "..."`

## Key Constraint

TOML-authored things are expected to be small, so parsing disk TOML into
`toml::Value` is acceptable for M3. The no-large-buffer rule remains important
for runtime/resource JSON payloads, but disk definitions do not need the same
streaming treatment in this milestone.

## Likely Shape

Keep authored files externally tagged by `kind`, but do not make `kind` a field
on every concrete slot record. Instead:

1. parse TOML text into `toml::Value`
2. require root table and root `kind`
3. map `kind` to the concrete slot shape id
4. remove or otherwise consume `kind` before applying the concrete record reader
5. call `registry.read_slot_toml(shape_id, &payload_without_kind)`
6. downcast to the concrete type and wrap it in `NodeDef`

This keeps the discriminator as wrapper metadata around the slotted payload.
The concrete def remains a normal slot record with only model fields.

## Open Questions

### Q1. Should the authored `kind` values stay lower-case domain names?

Context: current authored TOML uses `kind = "project"`, `kind = "texture"`,
etc. Slot shapes use Rust-derived shape ids/names. Switching authored TOML to
`kind = "ProjectDef"` would be mechanically tidy but user-hostile and a format
churn.

Suggested answer: keep the lower-case domain strings for authored node defs in
M3. They are the source-level language, while shape ids remain internal slot
schema identity.

### Q2. Should `kind` be stripped before applying SlotCodec?

Context: dynamic record reading correctly treats unknown fields as errors.
Concrete records such as `ProjectDef` do not have a `kind` field, and adding one
would make discriminator metadata look like model data.

Suggested answer: yes. The `NodeDef` wrapper consumes `kind`, then applies the
remaining table to the concrete slot record.

### Q3. Should ProjectBuilder switch to SlotCodec TOML writes in M3?

Context: the builder is not production loading, but it creates authored TOML
fixtures and currently uses serde serialization for model payloads.

Suggested answer: yes. This keeps generated test projects honest and catches
writer syntax drift early. Manual `kind` insertion can remain wrapper behavior.

## User Notes

- Use switch-it-and-fix-it.
- Keep serde derives and annotations temporarily.
- Definitions are M3 after messages.
- Keep future work in `future.md` so it is not lost.
- Simplicity beats serde-like cleverness; concrete models should stay simple
  slot records with explicit escape hatches only where needed.
