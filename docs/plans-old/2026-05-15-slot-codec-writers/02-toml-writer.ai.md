# Phase 2: TOML Writer

## Scope Of Phase

Implement generic shape-driven TOML writing in `lpc-model`.

In scope:

- add TOML value conversion for slot leaves
- add TOML shape walking over `SlotShape` + `SlotDataAccess`
- add registry APIs for writing root slot objects and arbitrary slot data to
  `toml::Value`
- add focused tests for records, maps, enums, options, refs, and value leaves

Out of scope:

- deleting old `lpc-wire/src/slot/authored_toml.rs`
- replacing project loading or real domain disk persistence

## Code Organization Reminders

- Keep TOML helpers in `dynamic_slot_writer.rs` if compact.
- Split into `toml_value_writer.rs` only if the file starts getting muddy.
- Put tests at the bottom of the file or in a focused module.
- Avoid duplicating large logic from `lpc-wire`; port the semantics into the
  new generic writer cleanly.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-core/lpc-model/src/slot_codec/dynamic_slot_writer.rs`
- `lp-core/lpc-model/src/slot_codec/slot_value_codec.rs`
- `lp-core/lpc-model/src/slot/slot_shape_registry.rs`
- `lp-core/lpc-wire/src/slot/authored_toml.rs` for old behavior reference only

Add functions similar to:

```rust
pub fn write_dynamic_slot_toml(
    registry: &SlotShapeRegistry,
    root: &dyn SlotAccess,
) -> Result<toml::Value, SlotDataWriteError>;

pub fn write_slot_data_toml_value(
    registry: &SlotShapeRegistry,
    id: SlotShapeId,
    data: SlotDataAccess<'_>,
) -> Result<toml::Value, SlotDataWriteError>;
```

TOML policy:

- records write tables
- maps write tables
- enum tables include `kind`, inserted before payload fields
- unit writes an empty table
- root `None` writes an empty table
- record `None` option fields are omitted
- present empty records/maps write empty tables

Leaf value conversion should support at least everything currently supported by
`write_lp_value`, including:

- string, bool, integer, f32
- vec/matrix arrays
- arrays/lists
- structs
- resource refs
- product refs

If resource/product TOML representation is not already obvious from
`slot_value_codec`, use the same explicit object fields as JSON:

- resource: `{ domain, id }`
- visual product: `{ kind = "visual", node, output }`
- control product: `{ kind = "control", node, output, preferred_extent = { ... } }`

Registry APIs:

- `SlotShapeRegistry::write_slot_toml`
- `SlotShapeRegistry::write_slot_toml_data`

Tests:

- write dynamic record to TOML and read it back through `read_slot_toml`
- write enum payload with `kind`
- omit `None` fields
- root `None` returns empty table
- resource/product TOML leaf round trip
- shape/data mismatch reports a useful error

## Validate

```bash
cargo fmt -p lpc-model --check
cargo test -p lpc-model dynamic_slot_writer
cargo test -p lpc-model slot_value_codec
```
