# Phase 01 — SourceFileSlot type + codec

**Dispatch:** [sub-agent: yes, model: composer-2.5-fast, parallel: -]

## Scope of phase

Add authored **`SourceFileSlot`** in `lpc-model` with custom codec dispatch.

**In scope:**

- `SourceFileBacking` (`Path` / `Inline { extension, text }`)
- `SourceFileSlot` with `revision()`, `path_value()`, `inline_value()`
- Custom codec id `lp::slots::SourceFileCodec`
- TOML forms: shorthand string, `$path` table, extension-key inline table
- Wire into `custom_slot_codec.rs` (read / write json / write toml / snapshot)
- Export from `slots/mod.rs` and `lib.rs`
- `ValueReader::is_string_scalar()` for shorthand disambiguation
- Unit tests in `source_file.rs`

**Out of scope:** `SourceFileRef`, resolve/materialize, production `ShaderDef` migration.

## Implementation Details

### `slots/source_file.rs`

- `SOURCE_FILE_CODEC_ID` = `SlotShapeId::from_static_name("lp::slots::SourceFileCodec")`
- Reserved key: `$path` (not `path`)
- Inline table: exactly one extension key (any non-reserved string)
- `FieldSlot` + `SlotCustomAccess` impls (mirror `NodeInvocation` pattern)

### `slot_codec/slot_reader.rs`

Add `ValueReader::is_string_scalar()` using `peek_event()` so path shorthand
does not collide with inline table parsing.

### Tests

- Parse `./shader.glsl` shorthand
- Parse `$path = "./shader.glsl"`
- Parse `glsl = "void main() {}"`
- Round-trip path shorthand to TOML

## Validate

```bash
cargo test -p lpc-model source_file
cargo check -p lpc-model
```
