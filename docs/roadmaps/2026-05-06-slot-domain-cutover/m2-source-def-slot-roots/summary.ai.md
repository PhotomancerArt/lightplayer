# M2 Summary: Source Def Slot Roots

## What Changed

M2 converted the authored source node definitions into slot-accessible roots while
keeping them usable as the source TOML model:

- `ProjectDef`, `NodeInvocation`, `TextureDef`, `ShaderDef`, `ShaderParamDef`,
  `OutputDef`, `FixtureDef`, `GlslOpts`, `MappingConfig`, and `PathSpec` now
  expose slot shapes/data through the real `lpc-model` slot traits.
- `lpc-source` now uses `SlotRecord` derive plus build-script generated static
  shape bootstrap code, so source static shapes are registered by generated code
  instead of a manually maintained list.
- Source defs use typed slot wrappers (`ValueSlot`, `MapSlot`, `OptionSlot`,
  semantic slots such as `Dim2uSlot`, `Affine2dSlot`, `RelativeNodeRefSlot`,
  `SourcePathSlot`, and `RenderOrderSlot`) at the fields that are authored and
  syncable.
- `examples/basic` was updated to the new source shape:
  - no project `uid`,
  - texture `size` record,
  - fixture `transform` record,
  - fixture `mapping` enum with stable-key maps instead of array-shaped TOML.
- Engine, shared test builders, view placeholders, CLI project creation, and
  legacy wire tests were updated to consume the new source model.
- A source sync evidence test now loads the real `examples/basic` source defs,
  registers the generated source shapes, emits a full slot sync, and verifies
  nested project, shader, texture, output, fixture, enum, map, and shader param
  data through the generic slot path.

## Decisions Captured

- Source defs are not plain structs. Their fields are versioned/syncable domain
  slots, so explicit `.value()` and helper accessors are appropriate at runtime
  boundaries.
- Arrays are not part of the authored slot container model for source defs.
  Fixture mapping data uses stable map keys even when TOML syntax is a little
  noisier.
- Source static shape registration should be generated. Manual static shape
  lists are too easy to forget during this domain-model churn.
- Shape IDs are explicit semantic IDs owned by the model (`source.shader`,
  `source.fixture.mapping`, etc.), while generated bootstrap code handles the
  registry loading mechanics.
- `uid` is no longer part of current project artifacts. CLI dev/profile paths
  keep transitional remote project-key behavior by falling back to project name
  or directory name when old `uid` is absent.
- Legacy wire/detail code still exists, but it now compiles against the new
  source defs. Removing that surface remains a later milestone.

## Validation

Ran:

```bash
cargo fmt --check --package lpc-model --package lpc-source --package lpc-slot-codegen --package lpc-slot-macros --package lpc-wire --package lpc-view --package lpc-engine --package lpa-client --package lpa-server --package lp-cli
cargo test -p lpc-model --lib --tests
cargo test -p lpc-slot-codegen --lib --tests
cargo test -p lpc-source --lib --tests
cargo test -p lpc-wire --lib --tests
cargo test -p lpc-wire --test source_slot_sync -- --nocapture
cargo test -p lpc-view --lib --tests
cargo test -p lpc-engine --lib --tests
cargo test -p lp-cli --lib --tests --no-run
cargo check -p lpc-source --features schema-gen
cargo check -p lpc-shared
cargo check -p lpc-engine
cargo check -p lpa-client
cargo check -p lpa-server
cargo clippy -p lpc-source -p lpc-wire -p lpc-engine -p lpc-view -p lpc-shared -p lp-cli --all-targets -- -D warnings
cargo clippy -p lpc-model -p lpc-slot-codegen -p lpc-slot-macros -p lpa-client -p lpa-server --all-targets -- -D warnings
git diff --check
```
