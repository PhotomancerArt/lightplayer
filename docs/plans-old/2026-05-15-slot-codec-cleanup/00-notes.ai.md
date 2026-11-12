# Slot Codec Cleanup Notes

## Scope

Remove the old codec experiments and vestigial serialization machinery now that
the slot registry can create defaults and apply a syntax reader dynamically.
The cleanup should make the intended architecture obvious:

- syntax sources and semantic reader/writer live in `lpc-model::slot_codec`
- slot shapes plus factories are the source of truth for object loading
- mockup tests should exercise the generic registry path, not hand-coded mockup
  serialization
- old `SlotCodec` record-deserialization/codegen should go away unless a piece
  is deliberately retained as a low-level primitive helper

This plan is for cleanup and prep. It should not adopt the custom serializer in
the real app yet.

## User Notes

- Remove "all the old codec stuff and anything else vestigial."
- Plan before implementing because we need to see what can be removed and what
  prep work is needed first.
- The mockup exists to pressure a generic system. It should not contain hacky
  mockup-specific codec machinery.
- Code size matters. Avoid growing generated code; favor generic helpers driven
  by slot shape metadata.
- Everything persisted should be in the slot system.

## Current State

### Newer Generic Path

- `lpc-model/src/slot/slot_shape_registry.rs`
  - has `read_slot_json`, `read_slot_toml`, and `read_slot_from`
  - has `write_slot_json`, `write_slot_json_value`, `write_slot_toml`, and
    `write_slot_toml_data`
  - returns `Box<dyn SlotMutAccess>`
  - creates through `SlotFactory` and applies shape-driven reader data
- `lpc-model/src/slot_codec/dynamic_slot_reader.rs`
  - shape-driven dynamic read implementation
  - supports records, maps, enums, options, refs, unit, and value leaves
- `lpc-model/src/slot_codec/dynamic_slot_writer.rs`
  - shape-driven dynamic write implementation
  - writes streaming JSON and `toml::Value`
- `lpc-model/src/slot_codec/slot_value_codec.rs`
  - typed leaf read/write helpers for `LpValue`
  - now includes resources and products
- `lpc-slot-mockup/src/tests/dynamic_slot_codec.rs`
  - uses registry read APIs and downcasts static shape reads to concrete Rust
    types

### Low-Level Reader/Writer Foundation

- `lpc-model/src/slot_codec/json_syntax_source.rs`
- `lpc-model/src/slot_codec/toml_syntax_source.rs`
- `lpc-model/src/slot_codec/slot_reader.rs`
- `lpc-model/src/slot_codec/slot_writer.rs`
- `lpc-model/src/slot_codec/syntax.rs`

These are still the foundation. They are not the old system, but some names
still carry JSON-prototype aliases:

- `SlotJsonWriter`
- `SlotJsonValue`
- `SlotJsonWrite`
- `SlotJsonWriterError`
- `SlotJsonObject`
- `SlotJsonArray`

The aliases are compatibility names and can be removed or replaced after
callers move to neutral names.

### Old Static `SlotCodec` Path

- `lpc-model/src/slot_codec/slot_codec.rs`
  - defines `SlotCodec`
  - implements record-ish and container-ish read/write for `ValueSlot`,
    `MapSlot`, `OptionSlot`, `GlslOpts`
- `lpc-model/src/lib.rs`
  - re-exports `SlotCodec`
- `lpc-slot-codegen/src/render/slot_codecs.rs`
  - generates `SlotCodec` impls plus `read_*_slot_body` and
    `write_*_slot_body`
- `lpc-slot-codegen/src/lib.rs`
  - exposes `generate_slot_codecs`
- `lpc-slot-codegen/src/config.rs`
  - exposes `SlotCodecCodegenConfig`
- `lpc-slot-mockup/build.rs`
  - generates `generated_slot_codec.rs`
- `lpc-slot-mockup/src/lib.rs`
  - includes the generated codec module
- `lpc-slot-mockup/src/source/node_def.rs`
  - manually implements `SlotCodec` for wrapper enum and calls generated body
    helpers
- `lpc-slot-mockup/src/source/mapping.rs`
  - manually implements `SlotCodec` for enum-heavy mockup types
- `lpc-slot-mockup/src/tests/generated_shape_codec.rs`
  - exercises the static generated codec path

This is the main deletion target. The new dynamic registry read path should
replace these tests and call sites.

### Old Manual Mockup Experiment

- `lpc-slot-mockup/src/tests/manual_shape_codec.rs`
  - large hand-written mock domain and codec experiment
  - duplicates concepts now represented by real mockup slot types
  - likely delete outright after preserving any still-useful coverage in
    `dynamic_slot_codec.rs` or focused `slot_codec` unit tests

### Manual Native Stream Demo

- `lpc-slot-mockup/src/tests/native_stream.rs`
  - still useful as a focused test of the low-level reader/writer primitives
  - not a domain codec
  - can be moved or slimmed into `lpc-model::slot_codec` tests so the mockup
    crate contains only mock domain tests

### Old SlotData Disk/Wire Serialization

- `lpc-wire/src/slot/authored_toml.rs`
  - encodes/decodes generic `SlotData` to TOML
  - has its own TOML-specific shape walking and value handling
- `lpc-wire/src/slot/slot_data_json.rs`
  - writes generic `SlotData` as JSON
- `lpc-slot-mockup/src/tests/storage_codec.rs`
  - exercises those old `SlotData` TOML/JSON paths
- `lpc-engine/src/engine/project_read_stream.rs`
  - still uses `write_slot_data_json` for project-read response streaming

These are vestigial relative to the new object-loading direction. The writer
replacement now exists in `lpc-model`, so the remaining work is moving callers
and deleting the old exports/files.

## What Can Be Removed Directly

- `lpc-slot-mockup/src/tests/manual_shape_codec.rs`
  - after checking whether any assertions are unique enough to move
- `lpc-slot-mockup/src/tests/generated_shape_codec.rs`
  - after dynamic registry tests cover the same static object read cases
- `lpc-slot-codegen` codec generation:
  - `SlotCodecCodegenConfig`
  - `generate_slot_codecs`
  - `render/slot_codecs.rs`
  - tests that assert generated `SlotCodec` output
- `lpc-slot-mockup/build.rs` generation of `generated_slot_codec.rs`
- `lpc-slot-mockup/src/lib.rs` include of `generated_slot_codec`

## What Needs Prep Before Removal

- Move old `lpc-wire` writer callers to the new registry writer APIs:
  - `write_slot_json_value`
  - `write_slot_toml_data`
- Move any useful low-level reader/writer primitive tests out of mockup and
  into `lpc-model::slot_codec` tests.
- Replace `SlotCodec` uses in `BindingDef`, `BindingDefs`, `BindingEndpoint`,
  `LpValue`, mockup `NodeDef`, and mockup `MappingConfig`.
  - For value leaves, prefer `SlotValue` + `read_lp_value`/`write_lp_value`.
  - For object loading, prefer registry dynamic read.
  - For wrapper enums, either add generic enum-wrapper support or keep a very
    small explicit wrapper reader while the generic enum story is finalized.
- Decide whether to delete or temporarily quarantine `lpc-wire` old
  `SlotData` TOML/JSON serializers.
  - Deleting `slot_data_json.rs` currently requires updating
    `lpc-engine/src/engine/project_read_stream.rs`.
  - Deleting `authored_toml.rs` requires replacing `storage_codec.rs` coverage
    with registry read/write tests.

## Open Questions

### Q1. Should `SlotCodec` be deleted entirely in this cleanup?

Context: `SlotCodec` is now the old static/generated object codec surface. It
also happens to host a few leaf/container convenience impls.

Suggested answer: yes, delete the public `SlotCodec` trait and generated codec
path. Keep low-level reader/writer primitives and leaf value helpers, because
those are part of the new design.

### Q2. Should the mockup retain any manual codec demo tests?

Context: `manual_shape_codec.rs` and `native_stream.rs` were useful while the
streaming reader shape was being discovered. They now make the mockup noisy.

Suggested answer: delete `manual_shape_codec.rs`; move a slimmed
`native_stream`-style primitive reader/writer test into `lpc-model` if it still
covers something not covered by `slot_codec` unit tests.

### Q3. Is old `lpc-wire` SlotData TOML/JSON serialization in scope now?

Context: `authored_toml.rs` and `slot_data_json.rs` are old shape-to-`SlotData`
serializers. They are not the desired "registry creates object and applies
reader" path. But `slot_data_json.rs` is still used by project read streaming.

Suggested answer: yes. The replacement generic writer APIs now exist in
`lpc-model`, so the cleanup can move remaining callers and delete the old
`lpc-wire` serializers.

### Q4. What is the target name for the cleaned module?

Context: current `slot_codec` is a mix of current and old pieces. User likes
`SlotCodec` as a system name, but the trait named `SlotCodec` is vestigial.

Suggested answer: keep the module name `slot_codec`, but delete/avoid the
`SlotCodec` trait name for now. Public concepts should be concrete and
operation-focused:

- `SlotReader`
- `SlotWriter`
- `JsonSyntaxSource`
- `TomlSyntaxSource`
- `read_dynamic_slot`
- `write_slot_access` or `write_dynamic_slot`
