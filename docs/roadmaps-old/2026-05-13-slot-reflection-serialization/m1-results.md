# M1 Results: Mockup Native Storage/Wire Codec

Date: 2026-05-13

## What Landed

The first mockup-first vertical slice is implemented.

- `lpc-slot-mockup` source roots now follow the current production model shape
  more closely: project, shader, texture, output, fixture, nested invocations,
  bindings, output options, fixture mapping, path specs, keyed maps, options,
  and enum discriminators.
- `lpc-wire` now has a reusable slot-shape-driven authored TOML codec:
  - borrowed slot access -> `toml::Value`
  - owned `SlotData` -> `toml::Value`
  - `toml::Value` -> owned `SlotData`
  - explicit unknown field, missing field, map key, and enum discriminator
    errors
- The mockup now tests a disk-storage path:
  - each persisted artifact is a slot root
  - loader-owned top-level `kind` is injected outside the slot root
  - authored TOML decodes back through `SlotShape` metadata
- The mockup now tests a wire-storage path:
  - direct borrowed JSON writer emits the same owned `SlotData` shape that
    snapshots produce

## Persisted Root Policy

The experiment follows the production core-node pattern:

- `project.toml` -> `ProjectDef`
- `shader.toml` -> `ShaderDef`
- `texture.toml` -> `TextureDef`
- `output.toml` -> `OutputDef`
- `fixture.toml` -> `FixtureDef`

All five are slot roots in the mockup. Nested persisted concepts are data
inside those roots, not independently persisted rootless values.

Top-level loader metadata such as `kind` is intentionally not slot data. The
TOML codec accepts this only through an explicit ignored-field list at the root.
Domain fields not present in the slot shape are rejected.

## Working Codec Conventions

- Records are TOML tables.
- Maps are TOML tables keyed by authored string form.
- Enums are TOML tables with `kind = "<variant>"`.
- Enum payload fields live beside `kind`.
- Absent options decode as `None`.
- Present options decode as `Some(payload)`.
- Empty maps have a default empty table.
- `Unit` fields have a default unit value.
- Missing non-optional scalar/record/enum fields are errors.
- Unknown record fields are errors unless the caller explicitly ignores them
  for loader metadata.

## Deliberate Deviations

- The mockup keeps `MappingConfig::Disabled`, `MappingConfig::Square`, and
  `PathSpec::Manual` as extra enum variants. Production currently only uses
  `PathPoints` and `RingArray`, but the extra variants keep the mockup useful
  for enum-switching and discriminator pressure.
- `BindingDefs` remains a mock equivalent. It has the right structural
  pressure as nested authored data, but does not attempt to model every
  production binding behavior.
- The old `RingLampCounts` semantic value test remains in the mockup as legacy
  value-leaf pressure, while the refreshed fixture mapping now uses the
  production-like keyed `MapSlot<u32, ValueSlot<u32>>`.
- The native TOML writer emits empty tables for empty maps such as `bindings`.
  Current Serde-authored production TOML often omits some empty/default data.
  Slot metadata does not yet express "skip empty/default" policy.
- The disk TOML codec does not persist slot revisions. Tests normalize
  revisions before comparing decoded TOML with live snapshots. This matches
  authored-storage expectations, but it is an explicit difference from the
  wire `SlotData` JSON shape.

## Rough Points Before Production Adoption

- Default policy needs a first-class design. Today the codec can default unit,
  option, and map fields, but not arbitrary semantic defaults such as "default
  shader path" or "default fixture mapping".
- Skip-empty/default emission needs slot metadata or a caller policy before
  native TOML can exactly match hand-authored production files.
- Error paths are useful but not span-aware. If authored diagnostics become a
  user-facing priority, the TOML parser/value layer may need spans.
- The TOML scalar support is intentionally narrow: strings, integers, floats,
  bools, vec2/vec3 arrays, structs, lists, and arrays. Other `LpType` variants
  still need explicit policy before real adoption.
- This slice proves slot-data conversion, not typed hydration. Production still
  needs a decision about whether node loading consumes `SlotData` directly,
  generates slot hydrators, or keeps a small typed bridge.

## Validation

Focused validation passed:

```bash
cargo test -p lpc-slot-mockup
cargo test -p lpc-wire --test source_slot_sync
```
