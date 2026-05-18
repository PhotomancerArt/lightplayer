# Phase 1: Build NodeDef SlotCodec TOML Reader

## Scope Of Phase

In scope:

- replace or supplement `NodeDef::from_toml_str` with a registry-backed
  SlotCodec TOML reader
- consume root `kind` as `NodeDef` wrapper metadata
- route the remaining TOML table through `SlotShapeRegistry::read_slot_toml`
- downcast concrete decoded objects and wrap them in `NodeDef`
- add focused `lpc-model` tests for project, texture, shader, output, and
  fixture TOML reads

Out of scope:

- changing `ProjectLoader`
- switching ProjectBuilder writes
- removing serde derives or serde annotations
- adding schema versioning

## Code Organization Reminders

- Keep `NodeDef` wrapper parsing in `nodes/node_def.rs` unless it grows enough
  to deserve a dedicated sibling file.
- Tests stay at the bottom of the file.
- Prefer small helpers such as `kind_from_table`, `payload_without_kind`, and
  `downcast_node_def`.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant file:

- `lp-core/lpc-model/src/nodes/node_def.rs`

Current serde-owned pieces:

- `NodeDefKindProbe`
- `parse_variant<T: serde::de::DeserializeOwned>`

Expected behavior:

1. Parse the input with `toml::from_str::<toml::Value>`.
2. Require the root to be a TOML table.
3. Require `kind` to be a string.
4. Map `kind` to the concrete def shape id and expected downcast type.
5. Clone the table and remove `kind`.
6. Read the clone through `registry.read_slot_toml(shape_id, &payload)`.
7. Downcast the returned `Box<dyn SlotMutAccess>` to the concrete def.
8. Return the appropriate `NodeDef` variant.

Use lower-case authored domain kind strings:

- `project`
- `texture`
- `shader`
- `output`
- `fixture`

Keep errors friendly:

- missing `kind`
- non-string `kind`
- unknown `kind`
- SlotCodec TOML syntax/mutation errors
- downcast mismatch

## Validate

```bash
cargo test -p lpc-model node_def
cargo test -p lpc-model slot_codec
```
