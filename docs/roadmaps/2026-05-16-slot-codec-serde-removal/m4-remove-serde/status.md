# M4 Status

## Implemented

- Removed direct `serde`, `serde_json`, and `schemars` dependencies from
  `lpc-model`.
- Removed `schema-gen` from `lpc-model` and downstream feature edges that
  pointed at `lpc-model/schema-gen`.
- Removed serde derives, attrs, manual impls, and serde-only tests from
  `lpc-model`.
- Added `slot_codec::metadata_codec` for explicit JSON read/write of:
  - `SlotShapeRegistrySnapshot`
  - `SlotShapeEntry`
  - `SlotShape`
  - `SlotValueShape`
  - `SlotMeta`
  - `ValueEditorHint`
  - `LpType`
  - `SlotData`
- Added `ValueReader::i64` and `SlotValueWriter::i64` for revision metadata.

## Validation

- `cargo check -p lpc-model` passes.
- `cargo test -p lpc-model` passes.
- Search for direct serde model usage has no hits in `lpc-model` source or
  Cargo dependency declarations.

## Current Blocker

`lpc-source` and crates that depend on it still derive serde for source-era
types that contain `lpc-model` types such as `ChannelName`, `NodePropSpec`,
`Kind`, `Constraint`, `NodeName`, and `LpValue`.

That means `cargo check -p lpc-wire` and `cargo test -p lpc-slot-mockup` now
fail before reaching the wire slot metadata boundary. This is expected from
removing serde from `lpc-model`, but it means the next phase must either:

- move or rewrite the remaining source-era serde surfaces in `lpc-source`, or
- temporarily keep compatibility impls for selected legacy model types until
  `lpc-source` is retired or converted.

## Note On `toml/serde`

`lpc-model` still enables the `toml` crate's `serde` feature because `toml::Value`,
`toml::Table`, and `toml::from_str::<toml::Value>` are gated behind that feature
in `toml 0.9`. This is not a direct model serde surface, but it is still a
format-library feature dependency.
