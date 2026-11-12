# Phase 1: ControlMessage Slot Shape

## Scope Of Phase

In scope:

- Add a slotted `ControlMessage` type.
- Expose `TriggerEvent` as the first semantic use of `ControlMessage`.
- Implement typed conversion to/from `LpValue`.
- Add tests for construction, distinct sequences, and slot shape generation.

Out of scope:

- Button node.
- Radio node.
- Playlist node.
- Source/address/args fields.

## Code Organization Reminders

- Prefer one concept per file.
- Keep tests at the bottom of files.
- Avoid commented-out code except the intentional design comment for deferred `address`/`args` in
  docs; production Rust should not contain dead commented fields.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-core/lpc-model/src/control/mod.rs` (new)
- `lp-core/lpc-model/src/control/control_message.rs` (new)
- `lp-core/lpc-model/src/lib.rs`
- `lp-core/lpc-model/src/slot_shapes/...` or static shape registration paths if needed

Expected API:

```rust
pub struct ControlMessage {
    id: u32,
    seq: u32,
}

impl ControlMessage {
    pub fn new(id: u32, seq: u32) -> Self;
    pub fn id(&self) -> u32;
    pub fn seq(&self) -> u32;
}

pub type TriggerEvent = ControlMessage;
```

Expected `LpValue` representation:

```rust
LpValue::Struct {
    name: Some(String::from("ControlMessage")),
    fields: vec![
        (String::from("id"), LpValue::U32(id)),
        (String::from("seq"), LpValue::U32(seq)),
    ],
}
```

Expected `SlotValueShape`:

- Shape id: `"ControlMessage"` or similarly stable/searchable.
- Type: `LpType::Struct { name: Some("ControlMessage"), fields: ... }`.
- Fields in stable order: `id`, `seq`.
- The `id` field is the sentinel map key field, matching the existing `FluidEmitter` convention.

Tests:

- `control_message_round_trips_through_lp_value`
- `control_message_value_shape_has_minimal_fields`
- `trigger_events_with_different_seq_are_distinct`
- `control_message_rejects_missing_or_wrong_fields`

## Validate

```bash
cargo fmt --check
cargo test -p lpc-model control_message
cargo check -p lpc-model --no-default-features
```
