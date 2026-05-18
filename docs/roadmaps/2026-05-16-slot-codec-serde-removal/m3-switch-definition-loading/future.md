# Future Work

## Public Authored TOML Writer API

- **Idea:** expose a polished `NodeDef` authored TOML writer in `lpc-model`
  once the loader path is stable.
- **Why not now:** M3 only needs enough writer support to keep generated
  project-builder fixtures honest.
- **Useful context:** `ProjectBuilder` can start with a local helper; if other
  callers need authored writes, promote that helper.

## Project Builder SlotCodec Writes

- **Idea:** migrate `lpc-shared::ProjectBuilder` to write complete `NodeDef`
  artifacts through the slot-native authored TOML writer.
- **Why not now:** keep this separate from final serde cleanup so generated
  projects pressure the public read/write API before dependencies are removed.
- **Useful context:** builder output should round-trip through
  `NodeDef::read_toml` without manually stitching `kind` onto concrete records.

## Better TOML Error Spans

- **Idea:** preserve TOML parser spans through `TomlSyntaxSource` so authored
  definition errors can point to exact files/properties.
- **Why not now:** current `toml::Value` based source drops spans, and M3 is
  about replacing serde behavior first.
- **Useful context:** `slot_codec::SyntaxError` already has path/span fields;
  TOML just needs a source that can populate them.
