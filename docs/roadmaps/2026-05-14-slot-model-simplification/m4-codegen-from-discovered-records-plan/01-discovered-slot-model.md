# Phase 1: Discovered Slot Model

## Scope Of Phase

Create a shared discovered-record model in `lpc-slot-codegen` that can be used
by shape, view, and codec generation.

In scope:

- inspect `#[derive(SlotRecord)]` structs once
- record type path, type name, inferred function stem, and field names/types
- preserve existing shape and view generation behavior
- add focused codegen tests for discovered records and fields

Out of scope:

- changing generated codec output
- enum/discriminator generation
- deleting `mockup_source_codec_module()`

## Code Organization Reminders

- Prefer concept-sized helpers over growing a single parser block.
- Keep discovery structs near existing discovery code unless the file becomes
  too hard to read.
- If splitting files, use search-friendly names such as
  `discovered_slot_record.rs`.
- Put tests at the bottom of the file.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-core/lpc-slot-codegen/src/lib.rs`

Current discovery functions:

- `discover_static_registered_shapes`
- `discover_static_slot_views`
- `slot_view_fields`
- `infer_type_path`

Expected changes:

- Add a shared discovery function, for example:

```rust
fn discover_static_slot_records(
    src_dir: &Path,
) -> Result<Vec<DiscoveredSlotRecord>, SlotShapeCodegenError>
```

- Include at least:
  - `type_path`
  - `type_name`
  - source path
  - named public fields
  - slot field name after `#[slot(name = "...")]`
  - field type tokens or parsed type summary
  - `#[slot(enum)]` marker
- Update shape/view discovery to reuse or be trivially derived from this model.
- Preserve duplicate shape id checks for `SlotRecord` and `SlotValue` names.

Tests to add/update:

- Existing codegen tests around static slot record discovery should still pass.
- Add a test proving discovered field names and `#[slot(enum)]` metadata are
  captured.
- Add a test proving shape/view generation output is unchanged or equivalent.

## Validate

```bash
cargo test -p lpc-slot-codegen
cargo test -p lpc-model
```

