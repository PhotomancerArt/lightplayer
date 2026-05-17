# Slot Model Simplification TODO

Current checkpoint: generated mockup `SlotCodec` reads and writes discovered
`SlotRecord` fields without a skip hook. `BindingDefs`, `BindingDef`, and
`BindingEndpoint` now have real model-side `SlotCodec` impls. The mockup now
also has a real `NodeDef` wrapper enum with discriminator-based `SlotCodec`
dispatch for JSON and TOML reads plus JSON writes.

## Mockup Cleanup

- [x] Remove generated mockup field skipping; slotted fields are now read and
  written unless their field type lacks `SlotCodec`.
- [x] Add a real mockup `NodeDef` wrapper enum so discriminator dispatch is
  proven against domain-shaped mockup types instead of only synthetic tests.
- [x] Add generated mockup tests for `NodeDef` TOML reads, JSON round trips, and
  invalid-kind diagnostics.
- [x] Remove mockup-specific bundle/surface/helper generation from
  `lpc-slot-codegen`; the generator now emits generic record `SlotCodec` impls.
- [ ] Delete or retire `lpc-slot-mockup/src/tests/manual_shape_codec.rs` once its
  remaining coverage is confirmed redundant with `generated_shape_codec.rs`.
- [ ] Add a generated mockup test that round-trips non-empty real `BindingDefs`
  through a real node definition, not only the synthetic generated bundle.
- [ ] Keep the mockup focused on "author slot data, everything else works"; avoid
  adding new hand-coded codec behavior unless it represents a deliberate escape
  hatch.

## Discriminators And Surfaces

- [x] Model polymorphic loads as thin slot-modeled enums where the use case is real,
  especially a `NodeDef`-style enum/wrapper.
- [x] Remove `mockup_codec_surfaces()` from codegen.
- [ ] Decide whether concrete `read_project_def_json`-style helpers should
  parse through `NodeDef` and downcast when authored docs include `kind`.
- [ ] Keep concrete load helpers thin: they should delegate to `T: SlotCodec`
  or to a wrapper enum, not own parse logic.
- [ ] Decide whether top-level document surfaces need slot metadata or can remain
  small explicit API functions outside the slot model.

## Generated SlotCodec

- [x] Generate mockup record `SlotCodec` impls from discovered `SlotRecord`
  fields.
- [x] Add a mockup `NodeDef` `SlotCodec` impl that consumes `kind` and
  delegates to generic generated record body helpers.
- [x] Stop hand-rendering mockup `NodeDef`, `MappingConfig`, `PathSpec`, and
  generated bundle helpers from the generic codegen crate.
- [x] Move generated codec support from "mockup-specific experiment" toward a real
  codegen mode for any crate with discovered `SlotRecord`s.
- [ ] Generate slot enum codecs from real enum metadata instead of writing
  `NodeDef` dispatch manually in the mockup domain.
- [x] Remove string-embedded custom helpers from `lpc-slot-codegen`.
- [ ] Keep generated record code small: default + field loop + `SlotCodec`
  delegation. Watch monomorph/code size as this grows.
- [ ] Make required/default field behavior explicit in codegen instead of relying
  only on `Default` plus mutation.

## Value And Leaf Codec Work

- [x] Add the first untyped `LpValue` literal path for simple binding literals.
- [ ] Firm up untyped `LpValue` literal reading/writing. The current path is enough
  for simple binding literals but intentionally small.
- [ ] Decide whether `LpValue` authoring should support typed/discriminated forms
  for ambiguous or rich values, or only simple inline literals at first.
- [ ] Keep semantic leaf behavior on `SlotValue`/`ToLpValue`/`FromLpValue`; avoid
  per-leaf codec functions unless the type has genuinely custom syntax.
- [ ] Ensure resource/product values have explicit dedicated codec behavior before
  they appear in authored data.

## TOML Writing

- [ ] Add a slot-native TOML writer/output stream alongside the JSON writer.
- [ ] Prove TOML writing in the mockup for concrete node docs and `NodeDef`
  wrapper docs.
- [ ] Decide how the TOML writer handles table ordering, inline tables, arrays
  of tables, and discriminator placement.
- [ ] Ensure generated `SlotCodec` writes can target both JSON and TOML without
  branching in generated record code.
- [ ] Use TOML writing to compare current authored model shape against desired
  on-disk shape; record intentional deviations.

## Error Quality

- [x] Preserve clear discriminator errors with expected values.
- [ ] Improve path/span reporting where errors are currently created with empty
  paths, especially in `read_lp_value` and binding endpoint parsing.
- [ ] Add tests for duplicate mutually exclusive keys such as `{ ref, value }`,
  missing payloads, and invalid binding refs.

## Real Model Adoption

- [ ] Apply the generated `SlotCodec` path to real domain `SlotRecord`s outside the
  mockup once the mockup is smooth.
- [ ] Add real-model `SlotCodec` for the real `NodeDef` wrapper.
- [ ] Replace real disk-loading TOML paths with slot-native reading where practical.
- [ ] Replace real disk-writing TOML paths with slot-native writing where practical.
- [ ] Replace wire JSON serialization/deserialization with slot-native codecs.
- [ ] Remove serde from no-std core paths after custom disk/wire loading is proven.

## Architecture Cleanup

- [x] Add root value cursors: `SlotReader::value()` and `SlotWriter::value()`.
- [ ] Decide whether format-neutral writer names can fully replace JSON aliases, or
  whether aliases stay during migration.
- [ ] Keep codec primitives discoverable in `lpc-model/src/slot_codec`; do not
  spread primitive handlers through generated code.
- [ ] Revisit whether `SlotCodec` should be split into read/write traits only if
  code size or API pressure makes that worthwhile.
- [ ] Document the root/surface/wrapper distinction once the enum approach settles.

## Size And Validation

- [ ] Capture baseline binary/code-size numbers before replacing serde in real
  no-std paths.
- [ ] Measure generated source size and final binary size after codegen expands to
  real model types.
- [ ] Do a "minimize monomorphs" pass if generated `SlotCodec` impls start bloating
  firmware.
- [x] Keep targeted validation green after the latest mockup `NodeDef` change:
  - `cargo test -p lpc-model`
  - `cargo test -p lpc-slot-codegen`
  - `cargo test -p lpc-slot-mockup`
  - `cargo check -p lpc-model --no-default-features`
