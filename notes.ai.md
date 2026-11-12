# LightPlayer Serialization Roadmap Notes

Date: 2026-05-13
Branch: `feature/lightplayer-serialize`
Worktree: `/Users/yona/dev/photomancer/feature/lightplayer-serialize`

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

## Direction That Looks Most Promising

### Core idea

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

### Why this fits LightPlayer

- Most authored configuration is already slot-based.
- Field defaults, optionals, enums, maps, and key semantics are already domain
  concepts, not just serialization concerns.
- We want consistent behavior across TOML and JSON, especially on embedded.
- We care about code size enough that custom converters are justified.

## Likely Wins

### 1. Collapse many per-type deserializers into a small number of generic walkers

Right now each authored type gets its own monomorphized Serde/TOML decode path.
If conversion is driven by slot metadata, a generic record/map/enum/option
walker can serve many types.

### 2. Unify TOML and JSON behavior

A slot-root converter gives one domain conversion path with two syntax adapters:

- TOML -> syntax value -> slot data
- JSON -> syntax value -> slot data

That should reduce duplication and avoid format-specific drift.

### 3. Keep authored TOML without paying for full generic typed TOML loading

We do not necessarily need to abandon TOML as a file format to reduce binary
size. We may only need to abandon generic typed TOML deserialization.

## Risks And Tradeoffs

### 1. More handwritten conversion code

This is the main cost. We trade derive convenience for explicit domain logic.

### 2. Error reporting needs deliberate design

Serde gives us a lot "for free" today. A custom converter needs its own:

- missing field errors
- unknown field errors
- bad type errors
- key parse errors
- path/span context

### 3. We should avoid writing a full TOML parser unless necessary

There are two separate problems:

- syntax parsing
- domain conversion

The highest-value work is probably custom domain conversion first, not custom
TOML syntax parsing first.

## Possible Technical Shapes

### Option A: Keep `toml`, replace typed decode with slot-domain conversion

Path:

- parse TOML into `toml::Table` or `toml::Value`
- convert via slot metadata into `SlotData`
- hydrate typed records from `SlotData`

Pros:

- lowest migration risk
- preserves full TOML syntax support
- isolates the expensive part we think matters most

Cons:

- still pays for some TOML value machinery
- may leave size on the table

### Option B: Move to a slimmer embedded TOML parser plus slot-domain conversion

Candidate crates worth evaluating:

- `picotoml`
- `toml-span`

Pros:

- could trim parser/value overhead further
- still preserves TOML authoring

Cons:

- larger migration
- may require more manual conversion code immediately
- parser compatibility and maintenance need validation

### Option C: Domain-native slot codec for both JSON and TOML

Path:

- introduce domain "authored value" abstraction
- implement TOML and JSON decoders into that abstraction
- implement authored value -> slot data converter once

Pros:

- maximal architectural clarity
- minimal duplicate semantics across formats

Cons:

- largest up-front design effort

## Recommendation

Start with a narrow experiment that validates the central premise before we
commit to a larger redesign.

### First experiment

Pick one hot authored type, likely one of:

- `NodeDef`
- `ShaderDef`
- `ComputeShaderDef`

Then:

1. Replace `toml::from_str::<T>` on that path with a custom slot-driven load.
2. Reuse existing slot metadata as much as possible.
3. Re-run `cargo bloat`.
4. Compare:
   - total firmware `.text`
   - `lpc_model`
   - `serde_core`
   - `toml`
   - `toml_parser`

If this produces a meaningful drop, proceed to a broader rollout.

## Proposed Rollout Phases

### Phase 0: Audit and target selection

- Inventory all on-device typed TOML entrypoints.
- Inventory all on-device typed JSON entrypoints.
- Rank by code size and call frequency.
- Choose the first migration target.

### Phase 1: Define the shared authored-value boundary

- Decide whether the common input layer is:
  - `toml::Value`-like
  - a custom "authored value" enum
  - direct walker callbacks
- Define common error/path reporting shape.

### Phase 2: Build generic slot-driven decode

- record walker
- map walker
- option walker
- enum walker
- scalar coercion by `LpType`
- key parsing using map slot key rules

### Phase 3: Adapt one hot TOML path

- convert one node definition path
- validate correctness
- re-measure size

### Phase 4: Expand across authored TOML loading

- migrate additional node defs
- migrate project-level loading
- remove now-unused per-type deserialize code where possible

### Phase 5: Reuse for JSON

- apply the same slot-domain conversion approach to JSON surfaces
- keep streaming writers where they are beneficial
- reduce typed `serde_json` usage where practical

## Questions To Answer Early

- Can typed records be hydrated directly from slot-domain data without
  reintroducing a large second layer of per-type generic code?
- How much of `SlotData` can become the canonical authored/runtime bridge?
- Which existing slot metadata is missing for complete authored decoding?
- Do we want one converter for authored config only, or a broader domain codec
  used by both storage and wire?
- Is TOML serialization on-device actually required, or only TOML parsing?

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
