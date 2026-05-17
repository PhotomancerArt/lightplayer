# Slot Reflection Serialization Notes

Date: 2026-05-13
Branch: `feature/lightplayer-serialize`
Worktree: `/Users/yona/dev/photomancer/feature/lightplayer-serialize`

## Prior Agent Handoff

An untracked root-level `notes.md` contained the current raw research handoff.
The main findings are preserved and organized here so the work has a durable
home under `docs/roadmaps`.

## Goal

Reduce embedded firmware code size, especially in the server/project-loading
path, without compromising the on-device compiler or the current
filesystem-backed project authoring model.

The working hypothesis is that we are paying too much for generic
Serde-driven TOML and JSON conversion across a large set of domain structs,
especially in slot-heavy model types that already have their own metadata
system.

## Measured Size Findings

These numbers came from `cargo bloat` run from `lp-fw/fw-esp32` with:

```bash
cargo bloat \
  --target riscv32imac-unknown-none-elf \
  --profile release-esp32 \
  --features esp32c6,server \
  --crates -n 80
```

Top `.text` contributors:

- `lps_frontend`: `263.4 KiB`
- `naga`: `248.5 KiB`
- `lpc_model`: `245.7 KiB`
- `lpc_engine`: `210.7 KiB`
- `core`: `150.4 KiB`
- `lpvm_native`: `103.5 KiB`
- `lps_builtins`: `83.2 KiB`
- `fw_esp32`: `73.9 KiB`
- `lpa_server`: `67.7 KiB`
- `serde_core`: `52.2 KiB`
- `toml`: `20.8 KiB`
- `toml_parser`: `16.7 KiB`
- `lpfs`: `15.5 KiB`
- `serde_json`: `15.3 KiB`
- `ser_write_json`: `9.4 KiB`

The key point is that `lpc_model` is not large because of core runtime model
logic alone. Its biggest symbols are dominated by TOML and Serde-generated
typed deserialization paths.

Examples from `cargo bloat --filter lpc_model`:

- `toml::de::from_str::<lpc_model::nodes::node_def::NodeDef>`
- `Deserialize` for `ShaderDef`
- `Deserialize` for `ComputeShaderDef`
- `Deserialize` for `FixtureDef`
- `Deserialize` for `ProjectDef`
- `Deserialize` for `BindingDefs`
- `Deserialize` for `MappingConfig`
- `Deserialize` for `NodeInvocation`
- large `MapSlot<...>` visitors specialized for authored node data

This strongly suggests that the expensive part is generic typed model loading,
not merely the presence of the `toml` crate.

## Current Architectural Reading

Today this all makes sense structurally:

- The embedded server owns project loading.
- The project filesystem lives on device.
- TOML is a good authored format for project editing and fallback inspection.
- The same model types are used across authoring, loading, and wire surfaces.

That architecture is coherent, but it couples the embedded build to a broad
Serde/TOML conversion surface.

## Important Observation

The domain already has a strong metadata system:

- `SlotShape`
- `SlotShapeRegistry`
- `SlotRecordShape`
- `SlotData`
- `ValueSlot`
- `MapSlot`
- slot key/shape/value semantics

This is already close to a domain schema/reflection layer.

That means we may not need generic Serde derives for most authored node and
slot-heavy types on-device. We can instead lean into our own metadata and use
format-specific frontends that map into a common slot representation.

## Current Code Pointers

- `lp-core/lpc-model/src/slot/slot_shape.rs`: canonical slot shape tree.
- `lp-core/lpc-model/src/slot/slot_data.rs`: owned dynamic snapshot data.
- `lp-core/lpc-model/src/slot/slot_access.rs`: borrowed reflection access over
  typed or dynamic data.
- `lp-core/lpc-slot-macros/src/record.rs`: `SlotRecord` derive generating
  shape/access code.
- `lp-core/lpc-wire/src/slot/slot_data_json.rs`: direct borrowed slot JSON
  writer.
- `lp-core/lpc-wire/src/slot/slot_shape_registry_json.rs`: direct registry JSON
  writer.
- `lp-core/lpc-slot-mockup/src/tests/authored_serde.rs`: current authored TOML
  round-trip evidence and pressure case.
- `docs/roadmaps/2026-05-06-slot-domain-cutover/overview.md`: slot-domain
  cutover rationale.
- `docs/plans-old/2026-05-12-project-read-end-to-end-streaming/00-design.md`:
  recent direct-writer/streaming direction.

## Direction That Looks Most Promising

Treat TOML and JSON as thin surface syntaxes over a domain-owned intermediate
representation, likely centered on `SlotData` plus slot shape metadata.

Instead of:

- `toml::from_str::<BigTypedModel>()`
- `serde_json::from_str::<BigTypedModel>()`

Move toward:

1. Parse TOML or JSON into a thin generic syntax/value layer.
2. Convert that syntax tree into slot-oriented domain data using
   `SlotShape`/`SlotRecordShape` metadata.
3. Hydrate typed Rust structs from slot/domain data only where needed.

## Likely Wins

### Collapse Per-Type Deserializers

Right now each authored type gets its own monomorphized Serde/TOML decode path.
If conversion is driven by slot metadata, a generic record/map/enum/option
walker can serve many types.

### Unify TOML And JSON Behavior

A slot-root converter gives one domain conversion path with two syntax
adapters:

- TOML -> syntax value -> slot data
- JSON -> syntax value -> slot data

That should reduce duplication and avoid format-specific drift.

### Keep Authored TOML

We do not necessarily need to abandon TOML as a file format to reduce binary
size. We may only need to abandon generic typed TOML deserialization.

## Risks And Tradeoffs

### More Handwritten Conversion Code

This is the main cost. We trade derive convenience for explicit domain logic.

### Error Reporting Needs Deliberate Design

Serde gives us a lot for free today. A custom converter needs its own:

- missing field errors
- unknown field errors
- bad type errors
- key parse errors
- path/span context

### Avoid Writing A Full TOML Parser First

There are two separate problems:

- syntax parsing
- domain conversion

The highest-value work is probably custom domain conversion first, not custom
TOML syntax parsing first.

## Parser Options To Evaluate Later

Initial slice should probably keep `toml`.

Potential later parser/value options:

- current `toml` crate value/table parser
- `toml-span` if span-aware diagnostics become valuable
- `picotoml` or another small TOML parser if code size remains parser-heavy
- `serde-json-core`-style JSON parsing only for constrained embedded JSON
  surfaces, if protocol needs diverge from authored TOML

Do not switch parser crates before measuring typed-deserializer removal.

## Questions To Answer Early

- Can typed records be hydrated directly from slot-domain data without
  reintroducing a large second layer of per-type generic code?
- How much of `SlotData` can become the canonical authored/runtime bridge?
- Which existing slot metadata is missing for complete authored decoding?
- Do we want one converter for authored config only, or a broader domain codec
  used by both storage and wire?
- Is TOML serialization on-device actually required, or only TOML parsing?
- Can host builds keep Serde derives for tests/tooling while embedded builds use
  the slot codec without fragmenting the domain model?

## Near-Term Hypothesis

The likely best fit for LightPlayer is:

- keep TOML as an authored format
- stop relying on broad typed Serde/TOML deserialization on embedded
- lean into slot metadata as the domain schema
- build custom generic converters once and reuse them for TOML and JSON

That approach is more custom than off-the-shelf Serde, but the constraints here
justify it:

- embedded binary size pressure
- on-device project loading
- slot-dense domain model
- desire for common behavior across authored and wire surfaces

## Next Suggested Step

Create a tiny prototype loader for one representative node path and measure the
delta with `cargo bloat` before designing the full migration.
